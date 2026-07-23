//! 中枢 actor：唯一的 Swarm poll 点。
//!
//! 对比旧栈 `EventLoop` 的三点结构变化：
//! - 命令从「trait 对象责任链 + owned event 穿链」改为扁平枚举 match +
//!   oneshot 应答——协议数固定的内核里，责任链的开闭收益换不回间接成本；
//! - 入站流分支整体移出（Router 自己 poll `IncomingStreams`），select 只剩
//!   命令与 swarm 事件两分支；
//! - 事件出口拆成 watch（状态采样）与 bounded mpsc fan-out（必达边沿），
//!   不再有单一巨型 NodeEvent 通道。
//!
//! 快路径不经 actor：开流走 `libp2p_stream::Control`（等价 iroh 的
//! ReadOnlyMap 直达 muxer），状态读取走 watch。

mod queries;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use futures::StreamExt;
use libp2p::core::transport::ListenerId;
use libp2p::swarm::dial_opts::DialOpts;
use libp2p::swarm::{ConnectionId, SwarmEvent};
use libp2p::{PeerId, Swarm, identify, kad, ping};
#[cfg(not(wasm_browser))]
use swarmdrop_net_base::DiscoverySource;
use swarmdrop_net_base::{Addr, NatStatus, NodeAddr, NodeId, PathKind, ProtocolId};
use tokio::sync::{mpsc, oneshot, watch};
use tracing::{debug, info, warn};

use crate::behaviour::{Behaviour, BehaviourEvent};
use crate::config::EndpointConfig;
use crate::dht::{DhtCommand, DhtError};
use crate::endpoint::{AddrsInfo, ConnInfo, InfraRoles, RelayState};
use crate::error::{ConnectError, Error};
use crate::event::NetEvent;
use crate::lookup::{AddressLookup, LocalNodeInfo, resolve_all};

use self::queries::{PendingQueries, PendingQuery};

/// 订阅者事件队列深度。满时丢弃并计数（presence 有 watch_conns 差分兜底）。
const SUBSCRIBER_QUEUE: usize = 256;

pub(crate) enum ActorMessage {
    Connect {
        addr: NodeAddr,
        reply: oneshot::Sender<Result<ConnInfo, ConnectError>>,
    },
    Disconnect {
        node: NodeId,
        reply: oneshot::Sender<Result<(), Error>>,
    },
    AddAddrs {
        node: NodeId,
        addrs: Vec<Addr>,
        reply: oneshot::Sender<Result<(), Error>>,
    },
    /// 显式登记本节点的外部可达地址（公网 relay / 动态 WebRTC 地址）。
    AddExternalAddr {
        addr: Addr,
        reply: oneshot::Sender<Result<(), Error>>,
    },
    SetKeepAlive {
        node: NodeId,
        enabled: bool,
        reply: oneshot::Sender<Result<(), Error>>,
    },
    Subscribe {
        tx: mpsc::Sender<NetEvent>,
    },
    /// DHT 子命令（kad 查询，QueryId 挂账）。
    Dht(DhtCommand),
    /// 注册基础设施节点（bootstrap / LanHelper / 自建 relay）：
    /// 进地址簿 + 进 kad 路由表 + 拨号 + 按角色请求 reservation（断线后
    /// 经 identify 幂等重建）。
    AddInfraPeer {
        peer: NodeAddr,
        roles: InfraRoles,
        reply: oneshot::Sender<Result<(), Error>>,
    },
    /// 注销基础设施节点（`AddInfraPeer` 的对称面）：撤销 relay 意图、
    /// 清地址簿/kad、关 circuit listener、立刻断开（含中止在途拨号）。
    RemoveInfraPeer {
        node: NodeId,
        reply: oneshot::Sender<Result<(), Error>>,
    },
    /// AddressLookup 解析完成的回注（内部 loopback）。
    ConnectResolved {
        addr: NodeAddr,
        reply: oneshot::Sender<Result<ConnInfo, ConnectError>>,
    },
    /// bind 尾声注入 lookup 集合（构造依赖 Endpoint，晚于 actor spawn）。
    SetLookups(Vec<Box<dyn AddressLookup>>),
    Shutdown,
}

/// watch 写端集合（actor 是唯一写者）。
pub(crate) struct WatchSenders {
    pub addrs: watch::Sender<AddrsInfo>,
    /// 写入点是 autonat 事件（native only）——wasm 下无 autonat，保持 Unknown。
    #[cfg_attr(wasm_browser, expect(dead_code, reason = "wasm 无 autonat，无写入点"))]
    pub nat: watch::Sender<NatStatus>,
    pub conns: watch::Sender<BTreeMap<NodeId, ConnInfo>>,
    pub relays: watch::Sender<BTreeMap<NodeId, RelayState>>,
}

pub(crate) struct Actor {
    swarm: Swarm<Behaviour>,
    rx: mpsc::Receiver<ActorMessage>,
    /// connect 等待表：peer → 等待者（ConnectionEstablished / 拨号失败时应答）。
    dials: HashMap<PeerId, Vec<oneshot::Sender<Result<ConnInfo, ConnectError>>>>,
    /// 地址簿（M1 最小版：manual 注入；M2 扩展为带来源/时效的 AddressBook，
    /// 汇聚 mdns/identify/dht 各 push 源）。
    ///
    /// 为什么需要它：`Swarm::add_peer_address` 只是把 `NewExternalAddrOfPeer`
    /// 广播给各 behaviour——没有 behaviour 存储它就没有任何效果；dial 的候选
    /// 地址来自 behaviour 的 `handle_pending_outbound_connection`。内核不依赖
    /// 特定 behaviour 兼职地址簿（旧栈靠 kad 路由表兼职），自己维护。
    address_book: HashMap<PeerId, Vec<libp2p::Multiaddr>>,
    /// 活跃连接明细（一个 peer 可能同时有 TCP+QUIC / relay+direct 多条连接）。
    conns: HashMap<PeerId, Vec<(ConnectionId, ConnInfo)>>,
    subscribers: Vec<mpsc::Sender<NetEvent>>,
    /// 队列满被丢弃的事件数（诊断用）。
    dropped_events: u64,
    watches: WatchSenders,
    /// kad 查询挂账表。
    queries: PendingQueries,
    /// circuit listener → relay peer。reservation 幂等去重 + ListenerClosed
    /// 时上抛 RelayReservationLost（迁自旧栈 `relay_listeners`）。
    relay_listeners: HashMap<ListenerId, PeerId>,
    /// 承担 relay 角色的基础设施节点——identify 到达时幂等重建 reservation。
    infra_relay_peers: HashSet<PeerId>,
    /// pull 型地址解析源（bind 尾声注入）。
    lookups: Arc<Vec<Box<dyn AddressLookup>>>,
    /// 自发端（lookup 任务解析完回注用）。
    self_tx: mpsc::Sender<ActorMessage>,
    config: EndpointConfig,
    node_id: NodeId,
}

impl Actor {
    pub(crate) fn new(
        swarm: Swarm<Behaviour>,
        rx: mpsc::Receiver<ActorMessage>,
        watches: WatchSenders,
        self_tx: mpsc::Sender<ActorMessage>,
        config: EndpointConfig,
        node_id: NodeId,
    ) -> Self {
        Self {
            swarm,
            rx,
            dials: HashMap::new(),
            address_book: HashMap::new(),
            conns: HashMap::new(),
            subscribers: Vec::new(),
            dropped_events: 0,
            watches,
            queries: PendingQueries::default(),
            relay_listeners: HashMap::new(),
            infra_relay_peers: HashSet::new(),
            lookups: Arc::new(Vec::new()),
            self_tx,
            config,
            node_id,
        }
    }

    pub(crate) async fn run(mut self) {
        loop {
            tokio::select! {
                msg = self.rx.recv() => match msg {
                    None | Some(ActorMessage::Shutdown) => break,
                    Some(msg) => self.handle_message(msg),
                },
                ev = self.swarm.select_next_some() => self.handle_swarm_event(ev),
            }
        }
        // 关停：回掉所有 pending 等待者，drop Swarm（断开全部连接 + 关监听）
        for (_, waiters) in self.dials.drain() {
            for tx in waiters {
                let _ = tx.send(Err(ConnectError::Closed));
            }
        }
        debug!(dropped_events = self.dropped_events, "actor stopped");
    }

    fn handle_message(&mut self, msg: ActorMessage) {
        match msg {
            ActorMessage::Connect { addr, reply } => self.handle_connect(addr, reply),
            ActorMessage::Disconnect { node, reply } => {
                let _ = self.swarm.disconnect_peer_id(*node.as_peer_id());
                let _ = reply.send(Ok(()));
            }
            ActorMessage::AddAddrs { node, addrs, reply } => {
                let peer = *node.as_peer_id();
                for addr in &addrs {
                    self.record_addr(peer, addr.as_multiaddr().clone());
                }
                let _ = reply.send(Ok(()));
            }
            ActorMessage::AddExternalAddr { addr, reply } => {
                self.swarm.add_external_address(addr.as_multiaddr().clone());
                // 显式地址是组合根提供的可自证配置；Swarm 不保证为它回发
                // ExternalAddrConfirmed，因此此处同步到唯一状态视图。
                let changed = self.watches.addrs.send_if_modified(|info| {
                    if info.external.contains(&addr) {
                        false
                    } else {
                        info.external.push(addr);
                        true
                    }
                });
                if changed {
                    self.publish_addrs();
                }
                let _ = reply.send(Ok(()));
            }
            ActorMessage::SetKeepAlive {
                node,
                enabled,
                reply,
            } => {
                self.swarm
                    .behaviour_mut()
                    .keep_alive
                    .set_keep_alive(*node.as_peer_id(), enabled);
                let _ = reply.send(Ok(()));
            }
            ActorMessage::Subscribe { tx } => self.subscribers.push(tx),
            ActorMessage::Dht(cmd) => self.handle_dht(cmd),
            ActorMessage::AddInfraPeer { peer, roles, reply } => {
                let peer_id = *peer.id.as_peer_id();
                for a in &peer.addrs {
                    self.record_addr(peer_id, a.as_multiaddr().clone());
                    if roles.kad_server
                        && let Some(kad) = self.swarm.behaviour_mut().kad.as_mut()
                    {
                        kad.add_address(&peer_id, a.as_multiaddr().clone());
                    }
                }
                if roles.relay {
                    self.ensure_relay(peer_id);
                } else {
                    // 非 relay 角色也主动建连（kad server 的路由表活性）
                    let candidates = self.address_book.get(&peer_id).cloned().unwrap_or_default();
                    if !candidates.is_empty()
                        && let Err(e) = self
                            .swarm
                            .dial(DialOpts::peer_id(peer_id).addresses(candidates).build())
                    {
                        debug!(%peer_id, error = %e, "infra peer dial skipped");
                    }
                }
                let _ = reply.send(Ok(()));
            }
            ActorMessage::RemoveInfraPeer { node, reply } => {
                self.handle_remove_infra_peer(node);
                let _ = reply.send(Ok(()));
            }
            ActorMessage::ConnectResolved { addr, reply } => self.handle_connect(addr, reply),
            ActorMessage::SetLookups(lookups) => {
                self.lookups = Arc::new(lookups);
            }
            ActorMessage::Shutdown => unreachable!("handled in run()"),
        }
    }

    fn handle_dht(&mut self, cmd: DhtCommand) {
        // kad 未启用：所有命令立即报错
        macro_rules! kad_or_reject {
            ($reply:expr) => {
                match self.swarm.behaviour_mut().kad.as_mut() {
                    Some(kad) => kad,
                    None => {
                        let _ = $reply.send(Err(DhtError::QueryFailed("dht not enabled".into())));
                        return;
                    }
                }
            };
        }
        match cmd {
            DhtCommand::Bootstrap { reply } => {
                let kad = kad_or_reject!(reply);
                match kad.bootstrap() {
                    Ok(id) => self.queries.insert(id, PendingQuery::Bootstrap { reply }),
                    Err(e) => {
                        let _ = reply.send(Err(DhtError::QueryFailed(e.to_string())));
                    }
                }
            }
            DhtCommand::Put {
                key,
                value,
                ttl,
                reply,
            } => {
                let kad = kad_or_reject!(reply);
                let mut record = kad::Record::new(key.as_bytes().to_vec(), value);
                record.publisher = Some(*self.node_id.as_peer_id());
                // kad 的 expires 类型按 target 分叉：native = std Instant，
                // wasm = web_time Instant（与 n0_future::time::Instant 同源）
                #[cfg(not(wasm_browser))]
                {
                    record.expires = ttl.map(|ttl| std::time::Instant::now() + ttl);
                }
                #[cfg(wasm_browser)]
                {
                    record.expires = ttl.map(|ttl| n0_future::time::Instant::now() + ttl);
                }
                match kad.put_record(record, kad::Quorum::One) {
                    Ok(id) => self.queries.insert(id, PendingQuery::Put { reply }),
                    Err(e) => {
                        let _ = reply.send(Err(DhtError::QueryFailed(e.to_string())));
                    }
                }
            }
            DhtCommand::Get { key, reply } => {
                let kad = kad_or_reject!(reply);
                let id = kad.get_record(kad::RecordKey::new(&key.as_bytes()));
                self.queries.insert(id, PendingQuery::Get { reply });
            }
            DhtCommand::Remove { key, reply } => {
                let kad = kad_or_reject!(reply);
                kad.remove_record(&kad::RecordKey::new(&key.as_bytes()));
                let _ = reply.send(Ok(()));
            }
            DhtCommand::Provide { key, reply } => {
                let kad = kad_or_reject!(reply);
                match kad.start_providing(kad::RecordKey::new(&key.as_bytes())) {
                    Ok(id) => self.queries.insert(id, PendingQuery::Provide { reply }),
                    Err(e) => {
                        let _ = reply.send(Err(DhtError::QueryFailed(e.to_string())));
                    }
                }
            }
            DhtCommand::StopProvide { key, reply } => {
                let kad = kad_or_reject!(reply);
                kad.stop_providing(&kad::RecordKey::new(&key.as_bytes()));
                let _ = reply.send(Ok(()));
            }
            DhtCommand::Providers { key, reply } => {
                let kad = kad_or_reject!(reply);
                let id = kad.get_providers(kad::RecordKey::new(&key.as_bytes()));
                self.queries.insert(
                    id,
                    PendingQuery::Providers {
                        found: Vec::new(),
                        reply,
                    },
                );
            }
        }
    }

    /// 确保经某 relay 的 reservation：**必须先与 relay 有活跃连接**才能
    /// listen circuit（旧栈实证的顺序）。未连接时先拨号，identify 到达后
    /// 经 `infra_relay_peers` 幂等触发真正的 circuit listen。
    ///
    /// 重试轮数不在此记账——那是上层策略（supervisor 退避）的内账，
    /// 机制层只登记意图、报告状态。
    fn ensure_relay(&mut self, peer_id: PeerId) {
        self.infra_relay_peers.insert(peer_id);
        // 已持有活跃 circuit listener：幂等 no-op
        if self.relay_listeners.values().any(|p| *p == peer_id) {
            return;
        }
        self.set_relay_connecting(peer_id);

        if self.conns.contains_key(&peer_id) {
            self.request_relay_reservation(peer_id);
            return;
        }
        let candidates = self.address_book.get(&peer_id).cloned().unwrap_or_default();
        if candidates.is_empty() {
            warn!(%peer_id, "no addresses for relay, cannot connect");
            self.set_relay_failed(peer_id, "no addresses for relay");
            return;
        }
        if let Err(e) = self
            .swarm
            .dial(DialOpts::peer_id(peer_id).addresses(candidates).build())
        {
            // DialPeerConditionFalse（在途拨号）无妨——identify 终会触发 reservation
            debug!(%peer_id, error = %e, "relay dial skipped");
        }
    }

    /// 注销基础设施节点：`ensure_relay`/`AddInfraPeer` 的逆操作。
    /// 完成后内核不再有任何针对该节点的重连或 reservation 重建路径。
    fn handle_remove_infra_peer(&mut self, node: NodeId) {
        let peer = *node.as_peer_id();
        self.infra_relay_peers.remove(&peer);
        self.address_book.remove(&peer);
        if let Some(kad) = self.swarm.behaviour_mut().kad.as_mut() {
            kad.remove_peer(&peer);
        }
        // 先摘表再关 listener——随后到达的 ListenerClosed 反查不到该 peer，
        // 不会误翻 Failed / 误发 RelayReservationLost
        let listeners: Vec<ListenerId> = self
            .relay_listeners
            .iter()
            .filter_map(|(id, p)| (*p == peer).then_some(*id))
            .collect();
        for id in listeners {
            self.relay_listeners.remove(&id);
            self.swarm.remove_listener(id);
        }
        self.watches
            .relays
            .send_if_modified(|map| map.remove(&node).is_some());
        // 立刻断：established 优雅关闭 + pending 拨号中止
        //（pin 93c5059 `Pool::disconnect` 对 pending 连接调用 abort）
        let _ = self.swarm.disconnect_peer_id(peer);
        // 挂在该 peer 上的 connect 等待者立即应答，不再干等超时
        self.fail_dial_waiters(peer, "infrastructure peer removed");
        info!(%peer, "infrastructure peer removed");
    }

    /// 该 peer 的全部 connect 等待者立即以失败应答（拨号失败 / 注销清算共用）。
    fn fail_dial_waiters(&mut self, peer: PeerId, reason: &str) {
        if let Some(waiters) = self.dials.remove(&peer) {
            for tx in waiters {
                let _ = tx.send(Err(ConnectError::DialFailed(reason.to_string())));
            }
        }
    }

    /// 写 watch：值相等时不通知（renewal / 幂等重入不惊动订阅者——每次冗余
    /// 通知都会放大成全部 `relays_changed` 流的序列化 + JS 侧重渲染）。
    fn set_relay_state(&mut self, peer: PeerId, new: RelayState) {
        let node = NodeId::from_peer_id(peer);
        self.watches.relays.send_if_modified(|map| {
            if map.get(&node) == Some(&new) {
                return false;
            }
            map.insert(node, new);
            true
        });
    }

    /// 写 watch：该 relay 进入 Connecting（覆盖 Failed——identify 重建 /
    /// 新一轮尝试都经此翻回）。
    fn set_relay_connecting(&mut self, peer: PeerId) {
        self.set_relay_state(peer, RelayState::Connecting);
    }

    /// 写 watch：该 relay 进入 Failed。仍持有活跃 circuit listener 时 no-op——
    /// guard 查的是 actor 的权威事实源 `relay_listeners`（并行拨号失败不推翻
    /// 活跃 reservation），而非 watch 投影；ListenerClosed 路径翻转前已摘表，
    /// guard 自然放行，无需旁路。
    fn set_relay_failed(&mut self, peer: PeerId, error: impl Into<String>) {
        if self.relay_listeners.values().any(|p| *p == peer) {
            return;
        }
        self.set_relay_state(
            peer,
            RelayState::Failed {
                last_error: error.into(),
            },
        );
    }

    /// 本机经某 relay 的完整 circuit 可达地址（`<relay>/p2p-circuit/p2p/<本机>`）。
    /// 单一事实源：调用方（web/桌面）不再自行拼接。
    fn circuit_addr_for(&self, relay: PeerId) -> Addr {
        let first = self
            .address_book
            .get(&relay)
            .and_then(|addrs| addrs.first())
            .cloned()
            .unwrap_or_else(libp2p::Multiaddr::empty);
        Addr::from_multiaddr(
            circuit_base(first, relay)
                .with(libp2p::multiaddr::Protocol::P2p(*self.node_id.as_peer_id())),
        )
    }

    /// 幂等请求 relay reservation：relay client 未启用或该 relay 已有活跃
    /// circuit listener 时 no-op（迁自旧栈 `request_relay_reservations`）。
    fn request_relay_reservation(&mut self, peer_id: PeerId) {
        if !self.swarm.behaviour().relay_client.is_enabled() {
            self.set_relay_failed(peer_id, "relay client disabled");
            return;
        }
        if self.relay_listeners.values().any(|p| *p == peer_id) {
            debug!(%peer_id, "relay reservation already active, skip");
            return;
        }
        let addrs = self.address_book.get(&peer_id).cloned().unwrap_or_default();
        if addrs.is_empty() {
            warn!(%peer_id, "no addresses for relay, cannot request reservation");
            self.set_relay_failed(peer_id, "no addresses for relay");
            return;
        }
        let mut requested = false;
        for addr in addrs {
            let relay_addr = circuit_base(addr, peer_id);
            match self.swarm.listen_on(relay_addr.clone()) {
                Ok(listener_id) => {
                    self.relay_listeners.insert(listener_id, peer_id);
                    requested = true;
                    info!(%relay_addr, "requesting relay reservation");
                }
                Err(e) => warn!(%relay_addr, error = %e, "relay circuit listen failed"),
            }
        }
        if requested {
            // 覆盖写：identify 幂等重建路径要把 Failed 翻回 Connecting；
            // 此处必无活跃 listener（函数开头已 skip），不会覆盖 Active
            self.set_relay_connecting(peer_id);
        } else {
            // 全部候选地址 listen_on 都失败（逐条已 warn）
            self.set_relay_failed(peer_id, "circuit listen failed");
        }
    }

    fn handle_connect(
        &mut self,
        addr: NodeAddr,
        reply: oneshot::Sender<Result<ConnInfo, ConnectError>>,
    ) {
        let peer = *addr.id.as_peer_id();

        // 已连接：直接返回当前最优路径快照
        if let Some(info) = self.best_conn(peer) {
            let _ = reply.send(Ok(info));
            return;
        }

        // 候选地址记入地址簿（dial 与后续重连都能用）
        for a in &addr.addrs {
            self.record_addr(peer, a.as_multiaddr().clone());
        }

        // 候选 = 显式传入 + 地址簿既有；behaviour 侧（kad 路由表等）的候选经
        // DialOpts 默认的 extend_addresses_through_behaviour 自动补充。
        let candidates = self.address_book.get(&peer).cloned().unwrap_or_default();

        // 无候选且配置了 pull 型 lookup：先解析再回注（ConnectResolved）
        if candidates.is_empty() && !self.lookups.is_empty() {
            let lookups = self.lookups.clone();
            let node = addr.id;
            let self_tx = self.self_tx.clone();
            n0_future::task::spawn(async move {
                let resolved = resolve_all(&lookups, node).await;
                if resolved.is_empty() {
                    let _ = reply.send(Err(ConnectError::NoAddresses(node)));
                    return;
                }
                let _ = self_tx
                    .send(ActorMessage::ConnectResolved {
                        addr: NodeAddr::with_addrs(node, resolved),
                        reply,
                    })
                    .await;
            });
            return;
        }

        let opts = if candidates.is_empty() {
            DialOpts::peer_id(peer).build()
        } else {
            DialOpts::peer_id(peer).addresses(candidates).build()
        };

        match self.swarm.dial(opts) {
            Ok(()) => self.dials.entry(peer).or_default().push(reply),
            // 已有拨号在途（infra dial / 并发 connect）：挂等待表共享其结果，
            // ConnectionEstablished / OutgoingConnectionError 到达时统一应答
            Err(libp2p::swarm::DialError::DialPeerConditionFalse(_)) => {
                self.dials.entry(peer).or_default().push(reply);
            }
            Err(e) => {
                let _ = reply.send(Err(ConnectError::DialFailed(e.to_string())));
            }
        }
    }

    fn handle_swarm_event(&mut self, event: SwarmEvent<BehaviourEvent>) {
        match event {
            SwarmEvent::ConnectionEstablished {
                peer_id,
                connection_id,
                endpoint,
                num_established,
                ..
            } => {
                let addr = Addr::from_multiaddr(endpoint.get_remote_address().clone());
                let path = classify_path(&addr);
                let info = ConnInfo {
                    path,
                    addr,
                    rtt: None,
                };
                let prev_best = self.best_conn(peer_id).map(|c| c.path);
                self.conns
                    .entry(peer_id)
                    .or_default()
                    .push((connection_id, info.clone()));
                self.publish_conns();

                let node = NodeId::from_peer_id(peer_id);
                if u32::from(num_established) == 1 {
                    self.emit(NetEvent::PeerConnected { node, path });
                } else if let Some(prev) = prev_best {
                    let new_best = self.best_conn(peer_id).map(|c| c.path).unwrap_or(path);
                    if new_best != prev {
                        // 例：dcutr 打洞成功后 direct 连接建立，Relayed → Direct
                        self.emit(NetEvent::PathChanged {
                            node,
                            path: new_best,
                        });
                    }
                }

                if let Some(waiters) = self.dials.remove(&peer_id) {
                    for tx in waiters {
                        let _ = tx.send(Ok(info.clone()));
                    }
                }
            }
            SwarmEvent::OutgoingConnectionError {
                peer_id: Some(peer),
                error,
                ..
            } => {
                // 有消费者才格式化 DialError（断网时拨号失败成批出现，
                // 多数事件既无 connect 等待者也非 infra relay）
                let has_waiters = self.dials.contains_key(&peer);
                let is_infra_relay = self.infra_relay_peers.contains(&peer);
                if has_waiters || is_infra_relay {
                    let error_str = error.to_string();
                    self.fail_dial_waiters(peer, &error_str);
                    // infra relay 拨号失败翻 Failed——该事件是 peer 级（本次 dial
                    // 的全部候选地址已耗尽），符合「全地址耗尽才算失败」判据
                    if is_infra_relay {
                        self.set_relay_failed(peer, error_str);
                    }
                }
            }
            SwarmEvent::ConnectionClosed {
                peer_id,
                connection_id,
                num_established,
                ..
            } => {
                let node = NodeId::from_peer_id(peer_id);
                let prev_best = self.best_conn(peer_id).map(|c| c.path);
                if let Some(list) = self.conns.get_mut(&peer_id) {
                    list.retain(|(id, _)| *id != connection_id);
                    if list.is_empty() {
                        self.conns.remove(&peer_id);
                    }
                }
                self.publish_conns();

                if num_established == 0 {
                    self.emit(NetEvent::PeerDisconnected { node });
                } else if let (Some(prev), Some(now)) =
                    (prev_best, self.best_conn(peer_id).map(|c| c.path))
                    && now != prev
                {
                    self.emit(NetEvent::PathChanged { node, path: now });
                }
            }
            SwarmEvent::NewListenAddr { address, .. } => {
                let addr = Addr::from_multiaddr(address);
                // LanHelper：把私网监听地址登记为 external——两个作用：
                // ① identify 广播给局域网对端（发现「这里有个 helper」，
                //    迁自旧栈 maybe_announce_lan_helper_addr）；
                // ② relay server 的 reservation 应答必须携带自身 external
                //    地址，否则 client 侧报 NoAddressesInReservation 拒绝
                //    整个 reservation（master 实测）。
                // loopback 一并放行：仅本机可达，生产无害，测试环境必需。
                if self
                    .config
                    .relay_server
                    .as_ref()
                    .is_some_and(|s| s.announce_private_addrs)
                    && (addr.is_private_lan() || addr.is_loopback())
                {
                    self.swarm.add_external_address(addr.as_multiaddr().clone());
                }
                self.watches
                    .addrs
                    .send_modify(|info| info.listen.push(addr));
                self.publish_addrs();
            }
            SwarmEvent::ListenerClosed {
                listener_id,
                addresses,
                reason,
                ..
            } => {
                // listener 关闭不会另发 ExpiredListenAddr——其地址从 listen 视图移除
                //（注销 relay 时 circuit 地址随之消失）
                self.remove_listen_addrs(&addresses);
                // circuit listener 关闭 = reservation 失效；该 relay 无其余
                // listener 时翻 Failed（摘表在前，set_relay_failed 的 listener
                // guard 自然放行）+ 上抛 RelayReservationLost。注销路径
                //（RemoveInfraPeer）已先摘 relay_listeners，此处反查不到 → 静默
                if let Some(relay_peer) = self.relay_listeners.remove(&listener_id)
                    && !self.relay_listeners.values().any(|p| *p == relay_peer)
                {
                    let last_error = match &reason {
                        Ok(()) => "reservation closed".to_string(),
                        Err(e) => e.to_string(),
                    };
                    self.set_relay_failed(relay_peer, last_error);
                    self.emit(NetEvent::RelayReservationLost {
                        relay: NodeId::from_peer_id(relay_peer),
                    });
                }
            }
            SwarmEvent::ExpiredListenAddr { address, .. } => {
                self.remove_listen_addrs(std::slice::from_ref(&address));
            }
            SwarmEvent::ExternalAddrConfirmed { address } => {
                let addr = Addr::from_multiaddr(address);
                // 去重：已收录则不触发 watch / lookup 重发（send_if_modified 返回是否真变）
                let changed = self.watches.addrs.send_if_modified(|info| {
                    if info.external.contains(&addr) {
                        false
                    } else {
                        info.external.push(addr);
                        true
                    }
                });
                if changed {
                    self.publish_addrs();
                }
            }
            SwarmEvent::ExternalAddrExpired { address } => {
                let addr = Addr::from_multiaddr(address);
                self.watches
                    .addrs
                    .send_modify(|info| info.external.retain(|a| *a != addr));
                self.publish_addrs();
            }
            SwarmEvent::Behaviour(ev) => self.handle_behaviour_event(ev),
            other => debug!(?other, "swarm event"),
        }
    }

    fn handle_behaviour_event(&mut self, event: BehaviourEvent) {
        match event {
            BehaviourEvent::Ping(ping::Event { peer, result, .. }) => match result {
                Ok(rtt) => {
                    // 更新该 peer 连接的 rtt 采样
                    if let Some(list) = self.conns.get_mut(&peer) {
                        for (_, info) in list.iter_mut() {
                            info.rtt = Some(rtt);
                        }
                        // 定点更新 watch 里该 peer 的 rtt，不全表重建（高频事件）；
                        // 连接建立/关闭仍走 publish_conns 全表重建（低频）。
                        let node = NodeId::from_peer_id(peer);
                        self.watches
                            .conns
                            .send_if_modified(|map| match map.get_mut(&node) {
                                Some(info) => {
                                    info.rtt = Some(rtt);
                                    true
                                }
                                None => false,
                            });
                    }
                    self.emit(NetEvent::PingSuccess {
                        node: NodeId::from_peer_id(peer),
                        rtt,
                    });
                }
                Err(e) => self.emit(NetEvent::PingFailure {
                    node: NodeId::from_peer_id(peer),
                    error: e.to_string(),
                }),
            },
            BehaviourEvent::Identify(identify::Event::Received { peer_id, info, .. }) => {
                // 基础设施 relay 断线重连后，identify 是幂等重建 reservation 的时机
                if self.infra_relay_peers.contains(&peer_id) {
                    self.request_relay_reservation(peer_id);
                }
                let protocols = info
                    .protocols
                    .iter()
                    .filter_map(|p| ProtocolId::new(p.as_ref().to_owned()).ok())
                    .collect();
                self.emit(NetEvent::PeerIdentified {
                    node: NodeId::from_peer_id(peer_id),
                    agent: info.agent_version,
                    protocol: info.protocol_version,
                    addrs: info
                        .listen_addrs
                        .into_iter()
                        .map(Addr::from_multiaddr)
                        .collect(),
                    protocols,
                });
            }
            BehaviourEvent::Kad(kad::Event::OutboundQueryProgressed {
                id, result, step, ..
            }) => {
                self.queries.handle(id, result, &step);
            }
            BehaviourEvent::RelayClient(ev) => self.handle_relay_client_event(ev),
            #[cfg(not(wasm_browser))]
            BehaviourEvent::Mdns(ev) => match ev {
                libp2p::mdns::Event::Discovered(list) => {
                    // 按 peer 聚合，进地址簿 + 发现事件
                    let mut by_peer: HashMap<PeerId, Vec<Addr>> = HashMap::new();
                    for (peer, addr) in list {
                        self.record_addr(peer, addr.clone());
                        by_peer
                            .entry(peer)
                            .or_default()
                            .push(Addr::from_multiaddr(addr));
                    }
                    for (peer, addrs) in by_peer {
                        self.emit(NetEvent::Discovered {
                            node: NodeId::from_peer_id(peer),
                            addrs,
                            source: DiscoverySource::Mdns,
                        });
                    }
                }
                libp2p::mdns::Event::Expired(_) => {}
            },
            #[cfg(not(wasm_browser))]
            BehaviourEvent::Autonat(ev) => {
                // AutoNAT v2：任一地址确认可达 → Public；失败不判 Private
                //（单次失败证据不足，旧栈同语义）。已 Public 后每次探测成功不再
                // send（否则级联触发 DHT 重发布 + 前端 IPC）——仅值变化才写。
                if ev.result.is_ok() {
                    self.watches.nat.send_if_modified(|nat| {
                        if *nat == NatStatus::Public {
                            false
                        } else {
                            *nat = NatStatus::Public;
                            true
                        }
                    });
                }
                debug!(tested = %ev.tested_addr, ok = ev.result.is_ok(), "autonat probe");
            }
            other => debug!(?other, "behaviour event"),
        }
    }

    fn handle_relay_client_event(&mut self, event: libp2p::relay::client::Event) {
        match event {
            libp2p::relay::client::Event::ReservationReqAccepted {
                relay_peer_id,
                renewal,
                ..
            } => {
                let relay = NodeId::from_peer_id(relay_peer_id);
                let circuit_addr = self.circuit_addr_for(relay_peer_id);
                // renewal 时值相等 → set_relay_state 不发通知（周期性空通知消除）
                self.set_relay_state(relay_peer_id, RelayState::Active { circuit_addr });
                if !renewal {
                    info!(%relay_peer_id, "relay reservation accepted");
                }
                self.emit(NetEvent::RelayReservationAccepted { relay, renewal });
            }
            other => debug!(?other, "relay client event"),
        }
    }

    /// 从 listen 视图移除一批地址，有变化时 republish（ListenerClosed /
    /// ExpiredListenAddr 共用——两处失效路径同一套规则）。
    fn remove_listen_addrs(&mut self, removed: &[libp2p::Multiaddr]) {
        let changed = self.watches.addrs.send_if_modified(|info| {
            let before = info.listen.len();
            info.listen.retain(|a| !removed.contains(a.as_multiaddr()));
            info.listen.len() != before
        });
        if changed {
            self.publish_addrs();
        }
    }

    /// 地址进簿（去重）+ 广播给 behaviour（kad 等各自决定是否收录）。
    fn record_addr(&mut self, peer: PeerId, addr: libp2p::Multiaddr) {
        let entry = self.address_book.entry(peer).or_default();
        if !entry.contains(&addr) {
            entry.push(addr.clone());
        }
        self.swarm.add_peer_address(peer, addr);
    }

    /// 一个 peer 多条连接时的最优路径（Local > Direct > Relayed）。
    fn best_conn(&self, peer: PeerId) -> Option<ConnInfo> {
        self.conns
            .get(&peer)?
            .iter()
            .map(|(_, info)| info)
            .max_by_key(|info| path_rank(info.path))
            .cloned()
    }

    fn publish_conns(&self) {
        let map: BTreeMap<NodeId, ConnInfo> = self
            .conns
            .keys()
            .filter_map(|peer| {
                self.best_conn(*peer)
                    .map(|info| (NodeId::from_peer_id(*peer), info))
            })
            .collect();
        self.watches.conns.send_replace(map);
    }

    /// 通知发布型 lookup（rendezvous / 在线宣告等自行 spawn 发布任务）。
    ///
    /// `watch_addrs` 是地址状态的唯一真相源——各 SwarmEvent 分支已 `send_modify`
    /// 更新它；这里只读当前值（`borrow().clone()`）转发给 lookup，不再持本地镜像。
    fn publish_addrs(&self) {
        if self.lookups.is_empty() {
            return;
        }
        let info = self.watches.addrs.borrow().clone();
        let local = LocalNodeInfo {
            node_id: self.node_id,
            addrs: info,
        };
        for lookup in self.lookups.iter() {
            lookup.publish(&local);
        }
    }

    /// 事件 fan-out：队列满丢弃并计数，订阅端关闭则移除。
    ///
    /// 末位订阅者 move 原值免 clone——本内核常态是唯一订阅者（core 事件循环），
    /// 该路径零 clone。
    fn emit(&mut self, event: NetEvent) {
        let Some(last) = self.subscribers.pop() else {
            return;
        };
        let dropped = &mut self.dropped_events;
        self.subscribers
            .retain(|tx| try_emit(tx, event.clone(), dropped));
        if try_emit(&last, event, dropped) {
            self.subscribers.push(last);
        }
    }
}

/// 向单个订阅者投递：满则丢弃计数（仍存活），关闭则返回 `false`（应移除）。
fn try_emit(tx: &mpsc::Sender<NetEvent>, event: NetEvent, dropped: &mut u64) -> bool {
    match tx.try_send(event) {
        Ok(()) => true,
        Err(mpsc::error::TrySendError::Full(ev)) => {
            *dropped += 1;
            warn!(?ev, "subscriber queue full, event dropped");
            true
        }
        Err(mpsc::error::TrySendError::Closed(_)) => false,
    }
}

pub(crate) fn subscriber_channel() -> (mpsc::Sender<NetEvent>, mpsc::Receiver<NetEvent>) {
    mpsc::channel(SUBSCRIBER_QUEUE)
}

/// circuit 基址归一化：确保携带 `/p2p/<relay>` 段后接 `/p2p-circuit`。
/// reservation listen 与 `Active` 状态下发共用（单一拼装规则，两处不漂移）。
fn circuit_base(addr: libp2p::Multiaddr, relay: PeerId) -> libp2p::Multiaddr {
    let base = if addr
        .iter()
        .any(|p| matches!(p, libp2p::multiaddr::Protocol::P2p(_)))
    {
        addr
    } else {
        addr.with(libp2p::multiaddr::Protocol::P2p(relay))
    };
    base.with(libp2p::multiaddr::Protocol::P2pCircuit)
}

/// 由连接的远端地址推断路径分类。
fn classify_path(addr: &Addr) -> PathKind {
    if addr.is_circuit() {
        PathKind::Relayed
    } else if addr.is_private_lan() || addr.is_loopback() {
        PathKind::Local
    } else {
        PathKind::Direct
    }
}

fn path_rank(path: PathKind) -> u8 {
    match path {
        PathKind::Local => 3,
        PathKind::Direct => 2,
        PathKind::Relayed => 1,
    }
}
