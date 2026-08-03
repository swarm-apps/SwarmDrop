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
use libp2p::swarm::dial_opts::{DialOpts, PeerCondition};
use libp2p::swarm::{ConnectionId, SwarmEvent};
use libp2p::{PeerId, Swarm, identify, kad, ping};
#[cfg(not(wasm_browser))]
use swarmdrop_net_base::DiscoverySource;
use swarmdrop_net_base::{Addr, NatStatus, NodeAddr, NodeId, PathKind, ProtocolId, TransportKind};
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

/// 一次 LAN 升级里，**每种传输**最多带几个候选地址。
///
/// **必须按传输分组，不能简单取前 N 个。** 原生端的 preset 同时监听
/// tcp / quic-v1 / webrtc-direct 三种，每种再乘以网卡数与 IPv4/IPv6——对端自报的
/// 私网地址轻易就有六条以上。而 webrtc-direct 是 listen 列表里最后注册的一种，
/// 于是「取前几个」截掉的往往正是它。
///
/// 那一刀砍在最要紧的地方：**浏览器拨不了裸 TCP/QUIC，webrtc-direct 是它够到
/// 局域网内原生端的唯一路径**。截掉它，「浏览器 ↔ 同网段的手机/桌面」这一格就
/// 永远升级不了，而那正是 Web 端最常见的局域网场景。
///
/// 分组取仍然挡得住原来要挡的东西：对端报一长串地址时，一次升级不会变成对内网的
/// 批量探测。
const LAN_UPGRADE_MAX_PER_TRANSPORT: usize = 2;

pub(crate) enum ActorMessage {
    Connect {
        addr: NodeAddr,
        request_id: u64,
        reply: oneshot::Sender<Result<ConnInfo, ConnectError>>,
    },
    /// 调用方的 connect 等待超时后清理对应等待者；若这是该 peer 唯一的
    /// 业务拨号且未承担基础设施角色，则同时中止底层 pending dial。
    CancelConnect {
        node: NodeId,
        request_id: u64,
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
    /// 运行期改写 identify 的 agent_version，并立刻向已连接对端主动 push。
    SetAgentVersion {
        agent_version: String,
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
    /// 撤销某 relay 的 reservation 意图：停止重建、移除 circuit listener。
    CancelRelayReservation {
        relay: NodeId,
        reply: oneshot::Sender<Result<(), Error>>,
    },
    /// 注销基础设施节点（`AddInfraPeer` 的对称面）。
    RemoveInfraPeer {
        node: NodeId,
        reply: oneshot::Sender<Result<(), Error>>,
    },
    /// AddressLookup 解析完成的回注（内部 loopback）。
    ConnectResolved {
        addr: NodeAddr,
        request_id: u64,
        reply: oneshot::Sender<Result<ConnInfo, ConnectError>>,
    },
    /// bind 尾声注入 lookup 集合（构造依赖 Endpoint，晚于 actor spawn）。
    SetLookups(Vec<Box<dyn AddressLookup>>),
    Shutdown,
}

/// actor 挂账的一次 connect 调用。显式 request_id 避免超时路径依赖
/// `oneshot::Sender::is_closed()` 的销毁时序。
struct ConnectWaiter {
    request_id: u64,
    reply: oneshot::Sender<Result<ConnInfo, ConnectError>>,
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
    dials: HashMap<PeerId, Vec<ConnectWaiter>>,
    /// 正在执行 AddressLookup 的 connect 请求；取消后移除，稍后回来的解析结果直接丢弃。
    resolving_connects: HashSet<(PeerId, u64)>,
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
    /// relay peer → 该 relay 上的 `/webrtc` 打洞监听器。
    ///
    /// **与 `relay_listeners` 分开存**：它不是一份 reservation（webrtc-p2p transport 收下
    /// 这个地址只是登记 listener，不向 relay 请求任何东西），混进去会让它的 ListenerClosed
    /// 被误判成 reservation 失效。
    webrtc_listeners: HashMap<PeerId, ListenerId>,
    /// 正在尝试局域网直连升级的对端（去重，避免 identify / mDNS 每次到达都重拨）。
    upgrading_lan: HashSet<PeerId>,
    /// 正在尝试打洞升级的对端。**与 `upgrading_lan` 分开存**，理由见
    /// [`Actor::clear_upgrade_marks`]。
    upgrading_direct: HashSet<PeerId>,
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
            resolving_connects: HashSet::new(),
            address_book: HashMap::new(),
            conns: HashMap::new(),
            subscribers: Vec::new(),
            dropped_events: 0,
            watches,
            queries: PendingQueries::default(),
            relay_listeners: HashMap::new(),
            webrtc_listeners: HashMap::new(),
            upgrading_lan: HashSet::new(),
            upgrading_direct: HashSet::new(),
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
            for waiter in waiters {
                let _ = waiter.reply.send(Err(ConnectError::Closed));
            }
        }
        debug!(dropped_events = self.dropped_events, "actor stopped");
    }

    fn handle_message(&mut self, msg: ActorMessage) {
        match msg {
            ActorMessage::Connect {
                addr,
                request_id,
                reply,
            } => self.handle_connect(addr, request_id, reply),
            ActorMessage::CancelConnect { node, request_id } => {
                self.cancel_connect(node, request_id)
            }
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
            ActorMessage::SetAgentVersion {
                agent_version,
                reply,
            } => {
                // agent_version 有两份副本，真值归属必须记牢：
                //   · identify `Behaviour` 内部那份是**权威**——新连接建立时
                //     handler 从它 clone 一份自用；
                //   · 这里的 `self.config.agent_version` 只是内核自己的诊断镜像
                //     （tracing / Debug 用），没有任何对外语义。
                // 两者必须由同一条命令一起更新，否则会长出「日志里的名字和线上广播
                // 的不一样」这种两边各自自洽、最难查的偏差。也正因为始终同步，这里
                // 可以拿镜像当幂等判据。
                //
                // 同值即空操作：不下发、不推送、不产生任何网络流量。少了这道短路，
                // 一次无意义的调用会让每个已连接对端都收到一条内容相同的 identify
                // push，白发一轮设备信息刷新。
                if self.config.agent_version != agent_version {
                    self.config.agent_version = agent_version.clone();

                    // 以下两步顺序写死，不可交换：
                    //   ① set_agent_version 逐连接压入 NotifyHandler::One(AgentVersionChanged)
                    //   ② push 压入 NotifyHandler::Any(Push)
                    // behaviour 的事件队列是 FIFO，先 set 后 push 才能让每条连接的 handler
                    // 在见到 Push 之前就已经持有新值——这是一条不依赖任何时序假设的保证。
                    //
                    // 反过来写今天**碰巧**也成立，但那是在赌 handler 的实现细节：
                    // `InEvent::Push` 只是排一次 OutboundSubstreamRequest，真正读
                    // `self.agent_version` 的 `build_info()` 要等子流协商完才跑，
                    // 那时 AgentVersionChanged 早到了。这个赌注一旦输掉，失败是**静默**的：
                    // 对端收到一次「内容没变」的 push，两端日志都正常，名字就是不更新，
                    // 要等 5 分钟后的周期交换才被纠正。集成测试也照不出来（它只能证明
                    // push 发生过），所以正确性只能靠这里的顺序守住。
                    self.swarm
                        .behaviour_mut()
                        .identify
                        .set_agent_version(agent_version);
                    let peers: Vec<PeerId> = self.conns.keys().copied().collect();
                    self.swarm.behaviour_mut().identify.push(peers);
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
            ActorMessage::CancelRelayReservation { relay, reply } => {
                self.cancel_relay_reservation(relay);
                let _ = reply.send(Ok(()));
            }
            ActorMessage::RemoveInfraPeer { node, reply } => {
                self.handle_remove_infra_peer(node);
                let _ = reply.send(Ok(()));
            }
            ActorMessage::ConnectResolved {
                addr,
                request_id,
                reply,
            } => {
                let peer = *addr.id.as_peer_id();
                if self.resolving_connects.remove(&(peer, request_id)) {
                    self.handle_connect(addr, request_id, reply);
                }
            }
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
        // reservation 依赖到 relay 的底层连接持续存在。Ping 被 keep-alive 语义排除，
        // 因而不能仅靠其 30s 探测阻止 Swarm 在 idle_connection_timeout 后回收该连接。
        self.swarm
            .behaviour_mut()
            .keep_alive
            .set_keep_alive(peer_id, true);
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
        self.swarm
            .behaviour_mut()
            .keep_alive
            .set_keep_alive(peer, false);
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
        self.remove_webrtc_listener(peer);
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
            for waiter in waiters {
                let _ = waiter
                    .reply
                    .send(Err(ConnectError::DialFailed(reason.to_string())));
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

    /// 与 circuit listener 同批挂一个 `<relay>/p2p-circuit/webrtc/p2p/<本机>` 监听（幂等）。
    ///
    /// 它让本机的可拨地址集里出现 `/webrtc` 变体——对端据此知道「这条路能打洞」，
    /// 拨过来时地址会被 webrtc-p2p transport 接走，走信令而非纯中转。
    ///
    /// ⚠️ **只在 [`Self::request_relay_reservation`] 里调，不要挪到
    /// `ReservationReqAccepted` 的处理路径上**——那条路径正是 libp2p relay client
    /// 更新自己 `reservation_addresses` 表的时刻，在其中插入 `listen_on` 会扰动它的
    /// 内部时序，把它的 `expect("Relay connection exist")` 打成 panic（浏览器实测
    /// 2/2 复现，短路本函数后 0/2）。相应地，撤销也只在
    /// [`Self::handle_remove_infra_peer`] 做，不跟随 `ListenerClosed`。
    ///
    /// 代价：reservation 掉线期间这条地址短暂不可达。可以接受——`request_relay_reservation`
    /// 幂等重试会把 reservation 拉回来，而让地址跟着每次抖动增删反而会让对端地址簿失效。
    fn ensure_webrtc_listener(&mut self, relay: PeerId) {
        if self.config.webrtc_p2p.is_none() || self.webrtc_listeners.contains_key(&relay) {
            return;
        }
        let Some(addr) = self
            .address_book
            .get(&relay)
            .and_then(|addrs| addrs.first())
            .cloned()
        else {
            return;
        };
        // 与 circuit_addr_for 同一套拼装规则，只多一个 /webrtc 段——格式由
        // webrtc-p2p 自己的构造函数保证，与它的 split 天然对称。
        let listen_addr = webrtc_p2p::protocol::addr::from_circuit(
            &circuit_base(addr, relay),
            *self.node_id.as_peer_id(),
        );
        match self.swarm.listen_on(listen_addr.clone()) {
            Ok(id) => {
                self.webrtc_listeners.insert(relay, id);
                info!(%listen_addr, "webrtc hole-punching listener registered");
            }
            Err(e) => warn!(%listen_addr, error = %e, "webrtc circuit listen failed"),
        }
    }

    /// 该 peer 当前是否只挂在中转上——两条升级路径共同的前提。
    fn only_relayed(&self, peer: PeerId) -> bool {
        self.conns
            .get(&peer)
            .is_some_and(|c| !c.is_empty() && c.iter().all(|(_, i)| i.path == PathKind::Relayed))
    }

    /// 清掉该 peer 的两个升级在途标记（连接建立 / 拨号失败时调）。
    ///
    /// **两条路径的标记必须分开存**：跨网时对端 identify 里那些私网地址本就拨不通，
    /// LAN 升级注定失败；若与打洞共用一个标记，一次失败就把打洞一起锁住，而
    /// identify 默认 5 分钟才来一轮——等于跨网场景永远打不了洞。
    fn clear_upgrade_marks(&mut self, peer: &PeerId) {
        self.upgrading_lan.remove(peer);
        self.upgrading_direct.remove(peer);
    }

    /// 把中转连接升级成**局域网直连**。
    ///
    /// **没有它，同一个局域网里的两台设备会一直经公网中继传文件。** 连接路径由
    /// 「谁先建成」定终身：presence 经 DHT 发现对端在线后立刻 `connect`，那一刻
    /// 地址簿里往往只有 circuit 候选（mDNS 还没到，或对端平台压根收发不了组播），
    /// relay 于是先赢；而 `handle_connect` 对已连接的 peer 直接返回当前快照，
    /// 之后再多的 `connect` 也不会重拨。
    ///
    /// 候选地址两个来源都汇到这里：
    /// - identify 的 `listen_addrs`（对端自报）——**不依赖 mDNS**，iOS/Android 把
    ///   组播挡掉时仍然有效，这是它比 mDNS 那条更要紧的原因；
    /// - mDNS `Discovered`（本机多播域实测），来得比 identify 快。
    ///
    /// **不做 [`should_initiate`] 那样的定序**，与打洞刻意不同：LAN 握手是毫秒级、
    /// 没有信令往返，两端各拨一次最坏只多一条 idle 后被回收的连接；而定序会让
    /// 「只有一端拨得通」（一端防火墙拦入站、一端 mDNS 瞎了）这类局域网里很常见的
    /// 情况彻底没救。打洞那边一次尝试是数秒 ICE + 信令往返，才值得定序。
    ///
    /// 安全性：拨的是**已过 Noise 认证的对端**自报的地址，libp2p 握手时校验 PeerId
    /// ——拨到网段内的其他机器只会失败。这也不新增信任面：`is_lan_discovered`
    /// （`PairingMethod::Direct` 的唯一授权依据）读的仍然只有 mDNS 来源的地址，
    /// 本函数一个字节都不往那张表里写。
    fn try_upgrade_to_lan(&mut self, peer: PeerId, candidates: &[libp2p::Multiaddr]) {
        if self.upgrading_lan.contains(&peer) || !self.only_relayed(peer) {
            return;
        }
        let lan = lan_candidates(candidates);
        if lan.is_empty() {
            return;
        }
        // `PeerCondition::Always`：此刻已经连着中转，默认条件会直接否掉这次拨号。
        let opts = DialOpts::peer_id(peer)
            .addresses(lan.clone())
            .condition(PeerCondition::Always)
            .build();
        match self.swarm.dial(opts) {
            Ok(()) => {
                self.upgrading_lan.insert(peer);
                info!(%peer, ?lan, "upgrading relayed connection to lan direct");
            }
            Err(e) => debug!(%peer, error = %e, "lan upgrade dial rejected"),
        }
    }

    /// 把中转连接升级成打洞直连——webrtc-p2p 版的 DCUtR。
    ///
    /// **没有它打洞永远不会发生**：webrtc-p2p 只在拨一个 `/webrtc` 地址时才启动信令，
    /// 而 libp2p 是并发拨号——candidate 里 circuit 与 `/webrtc` 一起拨，circuit 几乎
    /// 必然先成功（打洞要等 ICE 收敛数秒），于是那条根本没机会。所以必须在连上之后
    /// **主动再拨一次**。native 侧这活由 `dcutr` behaviour 干，浏览器侧没有对应物。
    ///
    /// 只在 identify 到达时调用：对端的 `/webrtc` 地址只经 identify 传来
    /// （`NewExternalAddrOfPeer` 不进地址簿），别处拿不到。
    fn try_upgrade_to_direct(&mut self, peer: PeerId, remote_addrs: &[libp2p::Multiaddr]) {
        if self.config.webrtc_p2p.is_none() || self.upgrading_direct.contains(&peer) {
            return;
        }
        // 对端通告了 `/webrtc` 变体 = 它支持打洞且可被拨。没有就是对端不支持，静默跳过。
        let Some(addr) = remote_addrs
            .iter()
            .find(|a| webrtc_p2p::protocol::addr::is_webrtc(a))
        else {
            return;
        };
        // 走到这里说明「本可以打洞」，后面每个否决点都留痕——否则「为什么没打洞」
        // 在一条有五个否决条件的链上根本无从查起（实测吃过这个亏）。
        if !self.only_relayed(peer) {
            debug!(%peer, "skip webrtc upgrade: already has a non-relayed path");
            return;
        }
        // `webrtc_listeners` 非空 = 本端也挂了 `/webrtc` 监听，即对端同样能拨过来
        if !should_initiate(
            self.node_id.as_peer_id(),
            &peer,
            !self.webrtc_listeners.is_empty(),
        ) {
            debug!(%peer, "skip webrtc upgrade: peer initiates (PeerId ordering)");
            return;
        }
        // `PeerCondition::Always`：此刻已经连着中转了，默认条件会直接否掉这次拨号。
        let opts = DialOpts::peer_id(peer)
            .addresses(vec![addr.clone()])
            .condition(PeerCondition::Always)
            .build();
        match self.swarm.dial(opts) {
            Ok(()) => {
                self.upgrading_direct.insert(peer);
                info!(%peer, %addr, "upgrading relayed connection via webrtc hole punch");
            }
            Err(e) => debug!(%peer, error = %e, "webrtc upgrade dial rejected"),
        }
    }

    /// 撤掉某 relay 上的 `/webrtc` 监听（reservation 失效 / 节点注销时调用）。
    fn remove_webrtc_listener(&mut self, relay: PeerId) {
        if let Some(id) = self.webrtc_listeners.remove(&relay) {
            self.swarm.remove_listener(id);
        }
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
        // **每个 relay 只申请一份 reservation**，哪怕地址簿里有它十几个地址。
        //
        // 曾对每个地址各 listen 一次，后果是把 relay 的配额吃光——它是 per-peer 的
        // （`max_reservations_per_peer` 默认 4），而一台 LanHelper 能通告十几个地址，
        // 于是多数请求以 `ResourceLimitExceeded` 被拒、reservation 反复丢失重建
        //（浏览器实测踩到，公网 relay 的总配额也被几个测试端占满）。
        //
        // 一份就够，因为走到这里时**必然已经连上 relay**（两个调用点都在 conns /
        // identify 之后，坑 5 的时序）：relay client 的 ListenReq 会走「复用现有连接」
        // 分支，我们传的地址只用来拼那条要通告的 external 地址，**不参与建连**。
        // 它的 `reservation_addresses` 又以 ConnectionId 为键——多份本就互相覆盖，
        // 最终生效的只有一份。
        let Some(addr) = self
            .address_book
            .get(&peer_id)
            .and_then(|addrs| addrs.first())
            .cloned()
        else {
            warn!(%peer_id, "no addresses for relay, cannot request reservation");
            self.set_relay_failed(peer_id, "no addresses for relay");
            return;
        };
        let relay_addr = circuit_base(addr, peer_id);
        match self.swarm.listen_on(relay_addr.clone()) {
            Ok(listener_id) => {
                self.relay_listeners.insert(listener_id, peer_id);
                info!(%relay_addr, "requesting relay reservation");
                // 覆盖写：identify 幂等重建路径要把 Failed 翻回 Connecting；
                // 此处必无活跃 listener（函数开头已 skip），不会覆盖 Active
                self.set_relay_connecting(peer_id);
                self.ensure_webrtc_listener(peer_id);
            }
            Err(e) => {
                warn!(%relay_addr, error = %e, "relay circuit listen failed");
                self.set_relay_failed(peer_id, "circuit listen failed");
            }
        }
    }

    /// 清除超时 connect 对应的等待者。libp2p 的
    /// `disconnect_peer_id` 会同时 abort pending connection；但同一 peer 可能有
    /// 多个调用者或承担 relay 基础设施角色，只有没有其他使用者时才允许中止。
    fn cancel_connect(&mut self, node: NodeId, request_id: u64) {
        let peer = *node.as_peer_id();
        self.resolving_connects.remove(&(peer, request_id));
        let no_waiters = self.dials.get_mut(&peer).is_some_and(|waiters| {
            waiters.retain(|waiter| waiter.request_id != request_id);
            waiters.is_empty()
        });
        if !no_waiters {
            return;
        }

        self.dials.remove(&peer);
        if !self.infra_relay_peers.contains(&peer) {
            // 返回 Err 仅表示还没有 established connection；pending dial 已被
            // libp2p 中止，故无需把它当成失败上抛。
            let _ = self.swarm.disconnect_peer_id(peer);
            debug!(%peer, "connect timed out; cancelled pending dial");
        }
    }

    /// 撤销 relay reservation。显式取消不是“reservation 丢失”，不发
    /// `RelayReservationLost`，避免上层把用户取消误判成需要自动恢复的故障。
    fn cancel_relay_reservation(&mut self, relay: NodeId) {
        let peer = *relay.as_peer_id();
        self.infra_relay_peers.remove(&peer);

        let listeners: Vec<_> = self
            .relay_listeners
            .iter()
            .filter_map(|(id, owner)| (*owner == peer).then_some(*id))
            .collect();
        for listener in listeners {
            self.relay_listeners.remove(&listener);
            let _ = self.swarm.remove_listener(listener);
        }
        self.watches
            .relays
            .send_if_modified(|relays| relays.remove(&relay).is_some());

        // 不干扰已建立的业务连接或其他 connect 调用；若只有 relay 自己遗留的
        // pending dial，disconnect_peer_id 会将它 abort，杜绝后续 reservation 重试。
        if !self.conns.contains_key(&peer) && !self.dials.contains_key(&peer) {
            let _ = self.swarm.disconnect_peer_id(peer);
        }
        debug!(%peer, "relay reservation cancelled");
    }

    fn handle_connect(
        &mut self,
        addr: NodeAddr,
        request_id: u64,
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
            self.resolving_connects.insert((peer, request_id));
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
                        request_id,
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
            Ok(()) => self
                .dials
                .entry(peer)
                .or_default()
                .push(ConnectWaiter { request_id, reply }),
            // 已有拨号在途（infra dial / 并发 connect）：挂等待表共享其结果，
            // ConnectionEstablished / OutgoingConnectionError 到达时统一应答
            Err(libp2p::swarm::DialError::DialPeerConditionFalse(_)) => {
                self.dials
                    .entry(peer)
                    .or_default()
                    .push(ConnectWaiter { request_id, reply });
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
                let path = classify_path(&addr, endpoint.is_relayed());
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

                // 本轮升级已有结果（无论这条是不是升级建出来的）；成功则 only_relayed
                // 转 false 自然不再重试，失败则等下一次 identify / mDNS 重来。
                self.clear_upgrade_marks(&peer_id);

                let node = NodeId::from_peer_id(peer_id);
                if u32::from(num_established) == 1 {
                    self.emit(NetEvent::PeerConnected {
                        node,
                        path,
                        addr: info.addr.clone(),
                    });
                } else if let Some(prev) = prev_best {
                    // path 与 addr 必须取自同一条连接（`best_conn` 的同一次快照），
                    // 分别求会在多连接并发建立时配出「Local + circuit 地址」这种组合。
                    let best = self.best_conn(peer_id).unwrap_or_else(|| info.clone());
                    if best.path != prev {
                        // 例：打洞成功后 direct 连接建立，Relayed → Direct
                        self.emit(NetEvent::PathChanged {
                            node,
                            path: best.path,
                            addr: best.addr,
                        });
                    }
                }

                if let Some(waiters) = self.dials.remove(&peer_id) {
                    for waiter in waiters {
                        let _ = waiter.reply.send(Ok(info.clone()));
                    }
                }
            }
            SwarmEvent::OutgoingConnectionError {
                peer_id: Some(peer),
                error,
                ..
            } => {
                // 判在清标记之前：这是「对端就在同一网段、我们拨了、没成」的唯一信号，
                // 上层拿它把一个沉默的「中继」徽标变成一句可行动的提示。
                //
                // 近似之处：`OutgoingConnectionError` 是 peer 级的，理论上可能来自同期
                // 另一次拨号。实践中升级在途时该 peer 极少有别的拨号（presence 不重拨
                // 已连接的 peer），且误报的代价只是多一句排障提示。
                if self.upgrading_lan.contains(&peer) {
                    self.emit(NetEvent::LanUpgradeFailed {
                        node: NodeId::from_peer_id(peer),
                    });
                }
                self.clear_upgrade_marks(&peer);
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
                cause,
                endpoint,
            } => {
                if let Some(error) = &cause {
                    warn!(
                        %peer_id,
                        %connection_id,
                        ?endpoint,
                        %error,
                        num_established,
                        "connection closed with error"
                    );
                } else {
                    debug!(
                        %peer_id,
                        %connection_id,
                        ?endpoint,
                        num_established,
                        "connection closed"
                    );
                }
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
                } else if let (Some(prev), Some(now)) = (prev_best, self.best_conn(peer_id))
                    && now.path != prev
                {
                    // 例：LAN 直连断了、只剩中转，Local → Relayed
                    self.emit(NetEvent::PathChanged {
                        node,
                        path: now.path,
                        addr: now.addr,
                    });
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
                    // 这里**刻意不撤** /webrtc listener：撤销要调 remove_listener，而这条
                    // 路径与 relay client 的内部状态机交错，实测会触发它的 panic
                    // （见 ensure_webrtc_listener）。reservation 会被幂等重试拉回来，
                    // 地址短暂不可达好过让对端地址簿反复失效。
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
                // 也是唯一能拿到对端 `/webrtc` 地址、进而升级中转连接的时机；
                // 对端的私网地址同样只经这里传来（mDNS 那条要看平台脸色）。
                //
                // 两条升级路径**都发起、互不阻塞**（标记分开存，见
                // `clear_upgrade_marks`）。LAN 那条通常毫秒级就赢，打洞那条随后
                // 建立的连接会因 path_rank 落选、idle 后自然回收——多一条短命连接，
                // 换的是「LAN 拨不通时不用再等 5 分钟才轮到打洞」。
                self.try_upgrade_to_lan(peer_id, &info.listen_addrs);
                self.try_upgrade_to_direct(peer_id, &info.listen_addrs);
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
            BehaviourEvent::RelayServer(libp2p::relay::Event::CircuitReqAccepted {
                src_peer_id,
                dst_peer_id,
            }) => {
                info!(%src_peer_id, %dst_peer_id, "relay circuit accepted");
            }
            BehaviourEvent::RelayServer(libp2p::relay::Event::CircuitReqDenied {
                src_peer_id,
                dst_peer_id,
                status,
            }) => {
                warn!(%src_peer_id, %dst_peer_id, ?status, "relay circuit denied");
            }
            BehaviourEvent::RelayServer(libp2p::relay::Event::CircuitClosed {
                src_peer_id,
                dst_peer_id,
                error: Some(error),
            }) => {
                warn!(%src_peer_id, %dst_peer_id, %error, "relay circuit closed with I/O error");
            }
            BehaviourEvent::RelayServer(libp2p::relay::Event::CircuitClosed {
                src_peer_id,
                dst_peer_id,
                error: None,
            }) => {
                info!(%src_peer_id, %dst_peer_id, "relay circuit closed");
            }
            BehaviourEvent::RelayServer(event) => {
                debug!(?event, "relay server event");
            }
            BehaviourEvent::WebrtcP2p(ev) => match ev {
                webrtc_p2p::Event::DirectConnectionEstablished { peer } => {
                    info!(%peer, "webrtc hole punching succeeded");
                }
                webrtc_p2p::Event::Failed { peer, error } => {
                    // 不是致命错误：relay 中转仍在，业务照常。spec 步骤 8 把
                    // 「打不通之后怎么办」明确留给应用，内核只如实记录。
                    debug!(%peer, %error, "webrtc hole punching failed, staying on relay");
                }
            },
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
                        // 多播域里看见了 = 对端确实和本机同网。此刻若还挂在中转上，
                        // 这是最早的升级时机——identify 要等到下一轮（默认 5 分钟）。
                        let candidates: Vec<libp2p::Multiaddr> =
                            addrs.iter().map(|a| a.as_multiaddr().clone()).collect();
                        self.try_upgrade_to_lan(peer, &candidates);
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

/// 由连接的远端地址与端点信息推断路径分类。
///
/// `relayed` 取自 [`ConnectedPoint::is_relayed`](libp2p::core::ConnectedPoint::is_relayed)，
/// **不能用 `addr.is_circuit()` 代替**：入站中继连接的 `send_back_addr` 只有
/// `/p2p/<src>` 一段（libp2p relay 就是这么填的），地址里根本没有 circuit 痕迹，
/// 单看地址会把中转判成直连——UI 显示反了，升级器也会以为「已有直连」而不再打洞。
/// `is_relayed()` 对入站看的是 `local_addr`，那里有 circuit 段。
fn classify_path(addr: &Addr, relayed: bool) -> PathKind {
    // WebRTC 打洞连接的地址形如 `<relay>/p2p-circuit/webrtc/p2p/<peer>`——含 circuit
    // 段只是沿用 libp2p 的地址约定（**信令**确实经 relay），数据面是打洞后的直连，
    // 一个字节都不过中继。故必须排在 `relayed` 之前判：否则打洞成功反而被记成
    // Relayed，既让 UI 显示与实情相反，也让 path_rank 把真直连排在中转之下。
    if is_hole_punched(addr) {
        return PathKind::Direct;
    }
    if relayed {
        return PathKind::Relayed;
    }
    if addr.is_private_lan() || addr.is_loopback() {
        PathKind::Local
    } else {
        PathKind::Direct
    }
}

/// 本端该不该主动发起打洞（`try_upgrade_to_direct` 的定序规则）。
///
/// 双方都可被拨时按 PeerId 定序只让一侧发起——两端同时打洞会建出两条多余连接
/// （spec 步骤 4 的「由 A 发起」正是为此）。
///
/// **`local_reachable` 那半边不能省**：本端没挂 `/webrtc` 监听时（没开打洞、或
/// reservation 还没起来），对端根本拨不过来，此时若还讲定序就会两边互等、谁都不发起。
fn should_initiate(local: &PeerId, remote: &PeerId, local_reachable: bool) -> bool {
    !local_reachable || local < remote
}

/// 从对端自报的地址里挑出 LAN 升级候选。
///
/// **每种传输各留几个**（`LAN_UPGRADE_MAX_PER_TRANSPORT`），而不是笼统取前 N 个——
/// 理由见那个常量：截掉 webrtc-direct 等于把浏览器那一格整个关掉。
fn lan_candidates(addrs: &[libp2p::Multiaddr]) -> Vec<libp2p::Multiaddr> {
    let mut taken: HashMap<Option<TransportKind>, usize> = HashMap::new();
    addrs
        .iter()
        .filter(|a| is_lan_candidate(a))
        .filter(|a| {
            let slot = taken
                .entry(Addr::from_multiaddr((*a).clone()).transport())
                .or_default();
            *slot += 1;
            *slot <= LAN_UPGRADE_MAX_PER_TRANSPORT
        })
        .cloned()
        .collect()
}

/// LAN 升级的候选判据：私网可达，且不是我们正想摆脱的那类 circuit 地址。
///
/// 排除 circuit 那半边不是多余的——LAN helper（局域网内的中继）自己就监听在
/// 私网地址上，它派发的 circuit 地址前半段同样 `is_private_lan()`，不排除就会
/// 把「换一条中继」当成「升级为直连」。
fn is_lan_candidate(addr: &libp2p::Multiaddr) -> bool {
    let addr = Addr::from_multiaddr(addr.clone());
    addr.is_private_lan() && !addr.is_circuit()
}

/// 是否为 webrtc-p2p 打洞建立的连接（circuit 地址里带 `/webrtc` 段）。
///
/// 判定复用 webrtc-p2p 自己的谓词——它同时是那个 transport 决定「收不收这个地址」的
/// 依据，两处若各写一份，改了一处另一处就会与实际接管关系错位。
///
/// 与 `/webrtc-direct` 是两回事：后者不含 circuit 段，走不到这里。
fn is_hole_punched(addr: &Addr) -> bool {
    webrtc_p2p::protocol::addr::is_webrtc(addr.as_multiaddr())
}

fn path_rank(path: PathKind) -> u8 {
    match path {
        PathKind::Local => 3,
        PathKind::Direct => 2,
        PathKind::Relayed => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RELAY: &str = "12D3KooWCkajTewJhupefZpVK7LwYfjG8bDJyXNtCgQYxiH1utep";
    const PEER: &str = "12D3KooWPjceQrSwdWXPyLLeABRXmuqt69Rg3sBYbU1Nft9HyQ6X";

    fn addr(s: &str) -> Addr {
        Addr::from_multiaddr(s.parse().expect("valid multiaddr"))
    }

    /// 打洞连接的地址天生带 circuit 段（信令经 relay），但数据面是直连。
    /// 判成 Relayed 会让 `path_rank` 把真直连排到中转之下，UI 也会显示反了。
    #[test]
    fn hole_punched_circuit_addr_ranks_as_direct() {
        let punched = addr(&format!(
            "/ip4/1.2.3.4/tcp/4001/p2p/{RELAY}/p2p-circuit/webrtc/p2p/{PEER}"
        ));
        let relayed = addr(&format!(
            "/ip4/1.2.3.4/tcp/4001/p2p/{RELAY}/p2p-circuit/p2p/{PEER}"
        ));

        assert_eq!(classify_path(&punched, true), PathKind::Direct);
        assert_eq!(classify_path(&relayed, true), PathKind::Relayed);
        assert!(
            path_rank(classify_path(&punched, true)) > path_rank(classify_path(&relayed, true)),
            "打洞成功后必须优于中转，否则最优连接会选错"
        );
    }

    /// 入站中继连接的 `send_back_addr` 只有 `/p2p/<src>`——地址里没有任何 circuit
    /// 痕迹，只能靠 `endpoint.is_relayed()` 判。曾因单看地址把它判成 Direct，
    /// 导致升级器以为「已有直连」而永不打洞。
    #[test]
    fn inbound_relayed_connection_is_not_direct() {
        let bare_peer = addr(&format!("/p2p/{PEER}"));
        assert_eq!(
            classify_path(&bare_peer, true),
            PathKind::Relayed,
            "地址无 circuit 段时必须信 endpoint"
        );
        assert_eq!(
            classify_path(&bare_peer, false),
            PathKind::Direct,
            "同一地址在非中继端点上仍是直连"
        );
    }

    /// LAN 升级只认「私网且非 circuit」。少了后半个条件，LAN helper 派发的
    /// circuit 地址（前半段也是私网）会被当成直连候选——那只是换了条中继。
    #[test]
    fn lan_upgrade_candidates_exclude_circuit_and_unreachable() {
        let lan = |s: &str| is_lan_candidate(&s.parse().expect("valid multiaddr"));

        assert!(lan("/ip4/192.168.1.5/tcp/4001"));
        assert!(lan("/ip4/10.0.0.7/udp/4001/quic-v1"));
        assert!(lan("/ip6/fd00::1/tcp/4001"));

        assert!(!lan("/ip4/127.0.0.1/tcp/4001"), "loopback 对端拨不到");
        assert!(!lan("/ip6/fe80::1/tcp/4001"), "link-local 需要 scope id");
        assert!(!lan("/ip4/47.115.172.218/tcp/4001"), "公网归打洞路径");
        assert!(
            !lan(&format!(
                "/ip4/192.168.1.9/tcp/4001/p2p/{RELAY}/p2p-circuit/p2p/{PEER}"
            )),
            "私网 LAN helper 的 circuit 地址不是直连，换中继不算升级"
        );
    }

    /// **浏览器那一格全靠这条**：浏览器拨不了裸 TCP/QUIC，webrtc-direct 是它够到
    /// 局域网内原生端的唯一路径；而 webrtc-direct 是 native preset 里最后注册的
    /// listener，对端自报的地址表里它排在最后。笼统「取前 N 个」正好把它截掉，
    /// 于是「浏览器 ↔ 同网段的手机」永远停在中继——症状是纯粹的「就是不升级」，
    /// 没有任何报错可查。
    #[test]
    fn lan_candidates_keep_one_of_every_transport() {
        // 一张网卡上的典型自报：三种传输 × IPv4/IPv6 ULA
        let addrs: Vec<libp2p::Multiaddr> = [
            "/ip4/192.168.1.5/tcp/54321",
            "/ip6/fd00::5/tcp/54321",
            "/ip4/192.168.1.5/udp/54322/quic-v1",
            "/ip6/fd00::5/udp/54322/quic-v1",
            "/ip4/192.168.1.5/udp/54323/webrtc-direct",
            "/ip6/fd00::5/udp/54323/webrtc-direct",
        ]
        .iter()
        .map(|s| s.parse().expect("valid multiaddr"))
        .collect();

        let picked = lan_candidates(&addrs);
        let kinds: Vec<_> = picked
            .iter()
            .map(|a| Addr::from_multiaddr(a.clone()).transport())
            .collect();

        assert!(
            kinds.contains(&Some(TransportKind::WebrtcDirect)),
            "webrtc-direct 被截掉 = 浏览器再也升级不了：{picked:?}"
        );
        assert!(kinds.contains(&Some(TransportKind::Tcp)));
        assert!(kinds.contains(&Some(TransportKind::Quic)));
    }

    /// 分组取仍要挡住「对端报一长串地址 → 一次升级变成对内网批量探测」。
    #[test]
    fn lan_candidates_cap_each_transport() {
        let addrs: Vec<libp2p::Multiaddr> = (1..=6)
            .map(|i| {
                format!("/ip4/192.168.1.{i}/tcp/4001")
                    .parse()
                    .expect("valid multiaddr")
            })
            .collect();

        assert_eq!(lan_candidates(&addrs).len(), LAN_UPGRADE_MAX_PER_TRANSPORT);
    }

    fn peer(seed: u8) -> PeerId {
        libp2p::identity::Keypair::ed25519_from_bytes([seed; 32])
            .expect("valid key")
            .public()
            .to_peer_id()
    }

    /// 定序规则：双方可拨时只有一侧发起，且**必是同一侧**。
    #[test]
    fn exactly_one_side_initiates_when_both_reachable() {
        let (a, b) = (peer(1), peer(2));
        assert_ne!(
            should_initiate(&a, &b, true),
            should_initiate(&b, &a, true),
            "两端得出相同结论会导致要么都不打洞、要么建两条连接"
        );
    }

    /// 本端不可被拨时必须无条件发起——否则对端也在等本端，双方互等谁都不动。
    #[test]
    fn unreachable_side_always_initiates() {
        let (small, big) = (peer(1), peer(2));
        assert!(should_initiate(&big, &small, false), "PeerId 较大也要发起");
        assert!(should_initiate(&small, &big, false));
    }

    /// webrtc-direct 是另一个传输：不含 circuit 段，本就走 Local/Direct 分支，
    /// 不该被 `/webrtc` 判定误伤。
    #[test]
    fn webrtc_direct_addrs_classify_by_reachability() {
        assert_eq!(
            classify_path(&addr("/ip4/192.168.1.9/udp/4001/webrtc-direct"), false),
            PathKind::Local
        );
        assert_eq!(
            classify_path(&addr("/ip4/47.115.172.218/udp/4003/webrtc-direct"), false),
            PathKind::Direct
        );
    }
}
