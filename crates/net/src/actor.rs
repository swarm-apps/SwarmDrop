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
    fn ensure_relay(&mut self, peer_id: PeerId) {
        self.infra_relay_peers.insert(peer_id);
        if self.conns.contains_key(&peer_id) {
            self.request_relay_reservation(peer_id);
            return;
        }
        let candidates = self.address_book.get(&peer_id).cloned().unwrap_or_default();
        if candidates.is_empty() {
            warn!(%peer_id, "no addresses for relay, cannot connect");
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

    /// 幂等请求 relay reservation：relay client 未启用或该 relay 已有活跃
    /// circuit listener 时 no-op（迁自旧栈 `request_relay_reservations`）。
    fn request_relay_reservation(&mut self, peer_id: PeerId) {
        if !self.swarm.behaviour().relay_client.is_enabled() {
            return;
        }
        if self.relay_listeners.values().any(|p| *p == peer_id) {
            debug!(%peer_id, "relay reservation already active, skip");
            return;
        }
        let addrs = self.address_book.get(&peer_id).cloned().unwrap_or_default();
        if addrs.is_empty() {
            warn!(%peer_id, "no addresses for relay, cannot request reservation");
            return;
        }
        let mut requested = false;
        for addr in addrs {
            let base = if addr
                .iter()
                .any(|p| matches!(p, libp2p::multiaddr::Protocol::P2p(_)))
            {
                addr.clone()
            } else {
                addr.clone().with(libp2p::multiaddr::Protocol::P2p(peer_id))
            };
            let relay_addr = base.with(libp2p::multiaddr::Protocol::P2pCircuit);
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
            self.watches.relays.send_modify(|map| {
                map.entry(NodeId::from_peer_id(peer_id))
                    .or_insert(RelayState::Connecting);
            });
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
                if let Some(waiters) = self.dials.remove(&peer) {
                    for tx in waiters {
                        let _ = tx.send(Err(ConnectError::DialFailed(error.to_string())));
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
            SwarmEvent::ListenerClosed { listener_id, .. } => {
                // circuit listener 关闭 = reservation 失效；该 relay 无其余
                // listener 时上抛 RelayReservationLost + 清 watch
                if let Some(relay_peer) = self.relay_listeners.remove(&listener_id)
                    && !self.relay_listeners.values().any(|p| *p == relay_peer)
                {
                    let relay = NodeId::from_peer_id(relay_peer);
                    self.watches.relays.send_modify(|map| {
                        map.remove(&relay);
                    });
                    self.emit(NetEvent::RelayReservationLost { relay });
                }
            }
            SwarmEvent::ExpiredListenAddr { address, .. } => {
                let addr = Addr::from_multiaddr(address);
                self.watches
                    .addrs
                    .send_modify(|info| info.listen.retain(|a| *a != addr));
                self.publish_addrs();
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
                self.watches.relays.send_modify(|map| {
                    map.insert(relay, RelayState::Active);
                });
                if !renewal {
                    info!(%relay_peer_id, "relay reservation accepted");
                }
                self.emit(NetEvent::RelayReservationAccepted { relay, renewal });
            }
            other => debug!(?other, "relay client event"),
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
