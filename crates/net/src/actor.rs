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
use std::time::Duration;

use futures::StreamExt;
use libp2p::core::transport::ListenerId;
use libp2p::swarm::dial_opts::{DialOpts, PeerCondition};
use libp2p::swarm::{ConnectionId, SwarmEvent};
use libp2p::{PeerId, Swarm, identify, kad, ping};
// 双 target 的 Instant：native 是 `tokio::time::Instant`（std 的薄封装），
// wasm 是 `web_time::Instant`（performance.now()）。
// **native 那半边不等于 std `Instant`**——kad 的 `Record::expires` 要的正是 std 那个，
// 所以 `handle_dht_command` 里那处仍得按 target 分叉，不能顺手换成这里的别名。
use n0_future::time::Instant;
#[cfg(not(wasm_browser))]
use swarmdrop_net_base::DiscoverySource;
use swarmdrop_net_base::{
    Addr, DialTier, NatStatus, NodeAddr, NodeId, PathKind, ProtocolId, TransportKind,
};
use tokio::sync::{mpsc, oneshot, watch};
use tracing::{debug, info, trace, warn};

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

/// 单个 peer 在地址簿里保留的地址上限。
///
/// # 为什么必须有上限
///
/// 地址簿是拨号候选的来源，而进簿的四条路径（mDNS / DHT presence record / identify /
/// 显式注入）**都会随时间产出新地址**：对端换 Wi-Fi 或 DHCP 续租就是一批新 IP，而带
/// `certhash` 的地址（webrtc-direct、WebTransport）更是每次证书轮换必变一条。只去重不
/// 淘汰的话，一个长期在线的对端会在簿里累积到几十上百条，其中绝大多数早已拨不通。
///
/// 后果与「本机通告的外部地址只增不删」是同一枚硬币的两面（见
/// [`Endpoint::set_external_addrs`](crate::Endpoint::set_external_addrs)）：那边撑爆的是
/// identify payload，这边占满的是拨号预算——libp2p 默认并发拨 8 条，死地址排在前面就
/// 意味着每次连接都要先超时几轮。
///
/// 32 的取法：一台设备的合理地址数在 10 条上下（TCP/QUIC 各 v4+v6、webrtc-direct、
/// WebTransport、多网卡，外加每个 relay 一条 circuit），留 3 倍余量。
const MAX_ADDRS_PER_PEER: usize = 32;

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

/// reservation **同步失败**后的重试闸门间隔：2s → 5s → 10s → 30s，上限 75s。
///
/// 档位与 core `InfraSupervisor::rebuild_backoff` 刻意取同一套。两者治的是同一件事的
/// 两层——那边管「还要不要维持这条链路的意图」，这边管「同一个同步失败要不要立刻重放」
/// ——档位分叉只会让两份日志的节奏对不上，排障时被当成两个独立问题追。
fn reservation_retry_backoff(attempts: u32) -> Duration {
    match attempts {
        0 | 1 => Duration::from_secs(2),
        2 => Duration::from_secs(5),
        3 => Duration::from_secs(10),
        4 => Duration::from_secs(30),
        _ => Duration::from_secs(75),
    }
}

/// 一个 relay 的 reservation 同步失败记账（[`Actor::relay_retry`] 的值）。
struct RelayRetry {
    /// 连续同步失败次数，只用来查 [`reservation_retry_backoff`] 的档位。
    attempts: u32,
    /// 早于此刻的尝试一律短路。
    retry_after: Instant,
    /// 上次失败原因。闸门短路时用它把 [`Actor::ensure_relay`] 刚翻上去的
    /// `Connecting` 压回 `Failed`——否则退避期内 UI 会停在「正在连接…」却说不出原因。
    last_error: String,
}

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
    /// 整份声明宿主认定的外部可达地址（幂等替换）。
    SetExternalAddrs {
        addrs: Vec<Addr>,
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
    address_book: HashMap<PeerId, Vec<AddrEntry>>,
    /// 地址簿的逻辑时钟：每次地址被上报或连通时递增一格，用来判断「谁更久没被提及」。
    /// 见 [`AddrEntry`] 关于「为什么不是时间戳」。
    addr_clock: u64,
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
    /// reservation **同步失败**后的重试闸门：peer → 记账（见 [`RelayRetry`]）。
    ///
    /// **这不是重试策略**——重试策略仍是 core `InfraSupervisor` 的内账（它决定还要不要
    /// 维持这条链路的意图），本表挡的是另一件事：`listen_on` 的同步失败与「地址簿里没有
    /// 可用 circuit 基址」都是**当前输入的确定性结果**，上层再催一遍必然得到同一个答案。
    ///
    /// 没有它时，mDNS 每刷新一次候选就把 supervisor 的退避清零
    ///（`candidate_seen` 重置），同一条注定失败的地址被以秒级频率重放几十次、每次刷一条
    /// warn（2026-08-10 真机实测）。轮数不外露、不进 `RelayState`，只用来算下次允许的时刻；
    /// 地址簿一有**能当 circuit 基址**的新条目就整条清掉（见 [`Actor::record_addr`]），
    /// 新事实到达不必等退避走完——而新进簿的 circuit 地址不算新事实，它对这两个失败原因
    /// 的答案与上一轮完全相同。
    relay_retry: HashMap<PeerId, RelayRetry>,
    /// **宿主声明的**外部可达地址（`SetExternalAddrs` 整份替换）。
    ///
    /// 与 [`confirmed_external`](Self::confirmed_external) 分开存，是为了让「声明」这个
    /// 动作可以是幂等的整份替换而不误伤自动发现的结果——合成一个集合的话，宿主每声明
    /// 一次就会把 AutoNAT 刚确认的地址抹掉，而下一轮 AutoNAT 再把它加回来，视图于是
    /// 永久抖动。
    declared_external: Vec<Addr>,
    /// **自动确认的**外部可达地址（AutoNAT / identify 观测，由 Swarm 事件驱动）。
    confirmed_external: Vec<Addr>,
    /// 由 [`EndpointConfig::external_ip`] 从监听集合映射出来的外部地址（未配置时恒空）。
    ///
    /// 独立一份而不并进 `declared_external`：它是内核**派生**的，随监听地址增删自动
    /// 重算；混进宿主声明的那份，宿主下一次整份声明就会把它抹掉，而下一条监听事件又把
    /// 它加回来 —— 视图于是永久抖动。判据见 [`Actor::recompute_mapped_external`]。
    mapped_external: Vec<Addr>,
    /// LanHelper 模式下作为 external 登记的**私网/回环监听地址**。
    ///
    /// 单独一份而不并进 `declared_external`：它们不属于「公网地址诊断」，不该出现在
    /// 视图里（那会让 UI 显示一串 192.168.x.x），但**必须**进 Swarm —— relay 的
    /// reservation 应答就靠它给客户端可拨地址，没有会被判 `NoAddressesInReservation`。
    lan_announced: Vec<Addr>,
    /// 当前**实际推给 Swarm** 的 external 集合 = 视图那份 ∪ [`lan_announced`]。
    ///
    /// 差量的基准是它而不是视图，理由见 [`resync_external`](Self::resync_external)。
    swarm_external: Vec<Addr>,
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
            addr_clock: 0,
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
            relay_retry: HashMap::new(),
            // 组合根声明的初值。Swarm 侧在 `bind` 里已登记过（reservation 应答要在
            // actor 起前就能带上公网地址），视图初值同样在那里给，故此处只建账本、
            // 不重复下发——`resync_external` 的差集因此从零开始就是空的。
            declared_external: config.external_addrs.clone(),
            confirmed_external: Vec::new(),
            // 监听地址此刻一条都还没到（`0.0.0.0` 尚未展开），故初值必为空——
            // 第一批 `NewListenAddr` 到达时才有内容。
            mapped_external: Vec::new(),
            lan_announced: Vec::new(),
            // 与 `bind` 里给的视图初值一致：那里已经把 `external_addrs` 推给 Swarm 了。
            swarm_external: crate::addrset::dedup_preserving_order(&config.external_addrs),
            lookups: Arc::new(Vec::new()),
            self_tx,
            config,
            node_id,
        }
    }

    /// 重算外部地址视图 = 宿主声明的 ∪ 自动确认的 ∪ 公网 IP 映射的，并把差量下发给 Swarm。
    ///
    /// **视图自身就是「上次同步了什么」的账本**，不另存第三份——多存一份就多一个会与
    /// 视图漂移的真值。
    ///
    /// ⚠️ **差量以 `swarm_external`（本 actor 自己的账本）为基准，不能拿视图代替。**
    /// 视图只装「公网地址诊断」那一部分（声明的 ∪ 自动确认的），而推给 Swarm 的集合还多
    /// 一类：LanHelper 把私网监听地址也登记为 external（`NewListenAddr` 分支），那是
    /// relay reservation 应答的可拨地址来源，刻意不进视图——混入私网地址会让 UI 显示得
    /// 莫名其妙。
    ///
    /// 拿视图当账本时那类地址**谁都撤不掉**：它从没进过视图，差量算不到它。以前无害
    /// （tcp/quic/webrtc-direct 的监听地址恒定），但 WebTransport 的地址带 certhash、
    /// 每 14 天变一次 —— 于是每轮换一次就多留一条死地址，随 identify 广播给每个对端。
    ///
    /// **通知 lookup 的动作交给调用方**（返回值就是「要不要通知」）：监听地址与外部地址
    /// 常常在同一个事件里一起变（`NewListenAddr` 会同时改监听视图和公网 IP 映射），
    /// 本函数自己 publish 的话那个事件就发两轮——而每轮都会连带触发一次 DHT publish。
    #[must_use = "视图变了就要 publish_addrs()，否则 lookup 拿到的是旧地址"]
    fn resync_external(&mut self) -> bool {
        let view_external = crate::addrset::union_preserving_order(
            &self.declared_external,
            &crate::addrset::union_preserving_order(
                &self.confirmed_external,
                &self.mapped_external,
            ),
        );
        // Swarm 侧还要带上 LanHelper 那部分。
        let desired = crate::addrset::union_preserving_order(&view_external, &self.lan_announced);

        for stale in self
            .swarm_external
            .iter()
            .filter(|a| !desired.contains(a))
            .cloned()
            .collect::<Vec<_>>()
        {
            self.swarm.remove_external_address(stale.as_multiaddr());
        }
        for fresh in desired.iter().filter(|a| !self.swarm_external.contains(a)) {
            self.swarm
                .add_external_address(fresh.as_multiaddr().clone());
        }
        self.swarm_external = desired;

        // 视图只在内容真变时才广播 —— 「没变」是常态（宿主每次重试都会把同一份声明重发
        // 一遍），白广播会连带触发一轮 DHT publish。
        self.watches.addrs.send_if_modified(|info| {
            if info.external == view_external {
                false
            } else {
                info.external = view_external;
                true
            }
        })
    }

    /// 按 [`EndpointConfig::external_ip`] 重算「监听地址的公网形态」，返回是否有变化。
    ///
    /// **不自己 `resync_external`**：它的两个调用点都还有别的 external 来源要同时变
    /// （`NewListenAddr` 可能同时登记 LanHelper 地址、`remove_listen_addrs` 可能同时撤销
    /// 它），各自 resync 一次就是把整轮差量算两遍、并可能对外广播两次「地址变了」。
    ///
    /// 每次都从 `listen` 视图整份重算，而不是按事件增删自己维护一份：视图是监听地址的
    /// 唯一真相源，整份派生天然不会与它漂移，也就没有「漏处理某条失效路径」这种缺陷
    /// ——`ListenerClosed` 与 `ExpiredListenAddr` 是两条独立路径，本仓已经在
    /// `lan_announced` 上踩过一次「只处理了其中一条」。
    ///
    /// 映射本身（含 circuit 排除与去重）在 [`crate::addrset::map_to_public_ip`]。
    fn recompute_mapped_external(&mut self) -> bool {
        let Some(ip) = self.config.external_ip else {
            return false;
        };
        // borrow 必须在 `resync_external` 之前放掉——那里要写同一个 watch。
        let mapped = {
            let info = self.watches.addrs.borrow();
            crate::addrset::map_to_public_ip(&info.listen, ip)
        };

        if mapped == self.mapped_external {
            return false;
        }
        self.mapped_external = mapped;
        true
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
            ActorMessage::SetExternalAddrs { addrs, reply } => {
                // 账本原样记「宿主声明了什么」，规范化（去重/排序）留给 `resync_external`
                // 的输出——两处都做等于有两份规范。
                self.declared_external = addrs;
                if self.resync_external() {
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
                    let candidates = self.peer_addrs(&peer_id);
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
        let candidates = self.peer_addrs(&peer_id);
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
        // 意图已撤销，闸门记账一并清掉——留着会让日后重新登记这个节点白等一轮退避
        self.relay_retry.remove(&peer);
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

    /// 该 relay 在地址簿里第一条**能当 circuit 基址**的地址（已拼好 `/p2p-circuit`）。
    ///
    /// 三个调用点（reservation listen / `/webrtc` 打洞 listen / 本机 circuit 可达地址）
    /// 共用它，而不是各自 `addrs.first()`：地址簿里混着 circuit 地址是常态（对端经第三方
    /// 中转可达时就会进来一条），`first()` 撞上那条就会拼出双层 circuit——判据与后果见
    /// [`circuit_base`]。
    fn circuit_base_for(&self, relay: PeerId) -> Option<libp2p::Multiaddr> {
        // 借着找，不物化整本簿 —— 它只要第一条能当基址的。
        first_circuit_base(
            self.address_book.get(&relay)?.iter().map(|e| &e.addr),
            relay,
        )
    }

    /// 本机经某 relay 的完整 circuit 可达地址（`<relay>/p2p-circuit/p2p/<本机>`）。
    /// 单一事实源：调用方（web/桌面）不再自行拼接。
    fn circuit_addr_for(&self, relay: PeerId) -> Addr {
        // 地址簿为空、或簿里只剩 circuit 地址时，退化成不带传输段的
        // `/p2p/<relay>/p2p-circuit/p2p/<本机>`。
        //
        // ⚠️ **这是展示值，不是可拨地址，不可当作可达地址分发给对端。** 此处此前写作
        // 「对端用自己认识的 relay 地址补前半段即可」——**libp2p 不做这件事**：缺前半段的
        // 地址进对端的 `relay::client::Transport::dial` 会以 `MissingRelayAddr` 当场被拒，
        // 而那个判别码同样落在 `TransportError::Other` 上、Display 是空串，对端日志里只会
        // 留下一条 `error=` 的空字段。它是一条死地址。
        //
        // 留着它是因为消费者只有状态展示（`RelayState::Active` → 节点状态弹窗的
        // 「circuit 可达地址」诊断与 `publicReachable` 布尔），而「有 reservation 却还没有
        // relay 的传输地址」是暂态：identify / mDNS 把地址报上来就补齐。真正要拨的那条走
        // `circuit_base_for`，那条路径没有这个退化分支。
        //
        // 不复用 `circuit_base(Multiaddr::empty(), …)`：空地址没有传输段，按判据它就该给
        // `None`（见 [`circuit_base`] 的第 2 条）。退化值只在这里显式拼一次。
        let base = self.circuit_base_for(relay).unwrap_or_else(|| {
            libp2p::Multiaddr::empty()
                .with(libp2p::multiaddr::Protocol::P2p(relay))
                .with(libp2p::multiaddr::Protocol::P2pCircuit)
        });
        Addr::from_multiaddr(
            base.with(libp2p::multiaddr::Protocol::P2p(*self.node_id.as_peer_id())),
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
        let Some(base) = self.circuit_base_for(relay) else {
            return;
        };
        // 与 circuit_addr_for 同一套拼装规则，只多一个 /webrtc 段——格式由
        // webrtc-p2p 自己的构造函数保证，与它的 split 天然对称。
        let listen_addr =
            webrtc_p2p::protocol::addr::from_circuit(&base, *self.node_id.as_peer_id());
        match self.swarm.listen_on(listen_addr.clone()) {
            Ok(id) => {
                self.webrtc_listeners.insert(relay, id);
                info!(%listen_addr, "webrtc hole-punching listener registered");
            }
            // `?e` 而非 `%e`：理由同 request_relay_reservation——TransportError 的
            // Display 在 `Other` 分支上是空串，`%` 会渲染出一条没有错误内容的日志。
            Err(e) => warn!(%listen_addr, error = ?e, "webrtc circuit listen failed"),
        }
    }

    /// 该 peer 当前是否只挂在中转上——两条升级路径共同的前提。
    /// 当前与该 peer 的所有连接里**最好的那一档**。没连接时 `None`。
    ///
    /// 这是所有「要不要升级」判断的唯一依据。它取代了此前的 `only_relayed`——那条判据
    /// 问的是「有没有直连」，而不是「有没有**更快的**直连」，于是一旦落到 webrtc-direct
    /// （`DirectSlow`）就再也不会往 `DirectFast` 升，永久锁死在慢传输上。
    fn best_tier(&self, peer: PeerId) -> Option<DialTier> {
        self.conns
            .get(&peer)
            .filter(|c| !c.is_empty())
            .and_then(|c| c.iter().map(|(_, i)| i.addr.dial_tier()).min())
    }

    /// 升级到 `target` 档是否有意义：当前已连上，且 `target` 严格更优。
    ///
    /// 未连接时返回 `false`——那时该走正常拨号，不是升级。
    fn wants_upgrade_to(&self, peer: PeerId, target: DialTier) -> bool {
        self.best_tier(peer).is_some_and(|current| target < current)
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
        if self.upgrading_lan.contains(&peer) {
            return;
        }
        let Some(current) = self.best_tier(peer) else {
            return;
        };
        // **只拨比现状更好的那一档，且只拨最好的那一档。**
        //
        // 「只拨更好的」把中继→直连与 webrtc-direct→WebTransport 收敛成同一条规则；
        // 「只拨一档」是因为 libp2p 并发拨号是延迟竞速——把 webrtc-direct 和
        // WebTransport 一起发出去，赢的多半是前者（后者要多一次 QUIC 握手），
        // 于是升级"成功"了，却落在慢的那一档上，而且从此 `current` 不再劣于任何候选，
        // 再也不会有第二次机会。
        let lan = lan_candidates(candidates, current);
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
        //
        // 打洞落在 `DirectSlow`，所以这条等价于旧的 `only_relayed`：已有任何直连时
        // 不再打洞。换成统一判据只是让「升级」这件事全域只有一处定义。
        if !self.wants_upgrade_to(peer, DialTier::DirectSlow) {
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
        // 同步失败后的重试闸门（见 `relay_retry`）。地址簿没变的前提下，重放必然
        // 得到同一个失败，唯一的产出是一条 warn——真机上被上层以秒级频率催了几十次。
        let backing_off = self
            .relay_retry
            .get(&peer_id)
            .filter(|retry| Instant::now() < retry.retry_after)
            .map(|retry| (retry.attempts, retry.last_error.clone()));
        if let Some((attempts, last_error)) = backing_off {
            debug!(%peer_id, attempts, "relay reservation backing off, skip");
            // `ensure_relay` 在调本函数前刚把状态翻成 Connecting；不压回去，退避期内
            // UI 会一直显示「正在连接…」，而真相是它压根没在试。
            self.set_relay_failed(peer_id, last_error);
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
        //
        // 取的是第一条**能当基址**的地址，不是第一条地址：地址簿里混着 circuit 地址时，
        // 拿它当基址会拼出双层 circuit（见 `circuit_base`）。
        let Some(relay_addr) = self.circuit_base_for(peer_id) else {
            warn!(%peer_id, "no usable circuit base address for relay, cannot request reservation");
            self.fail_relay_reservation(peer_id, "no usable relay address");
            return;
        };
        match self.swarm.listen_on(relay_addr.clone()) {
            Ok(listener_id) => {
                self.relay_listeners.insert(listener_id, peer_id);
                // 走到这一步说明地址是能用的，闸门记账作废
                self.relay_retry.remove(&peer_id);
                info!(%relay_addr, "requesting relay reservation");
                // 覆盖写：identify 幂等重建路径要把 Failed 翻回 Connecting；
                // 此处必无活跃 listener（函数开头已 skip），不会覆盖 Active
                self.set_relay_connecting(peer_id);
                self.ensure_webrtc_listener(peer_id);
            }
            Err(e) => {
                // `?e` 而非 `%e`：`TransportError` 的 Display 在 `Other` 分支上写的是
                // **空串**（libp2p `core/src/transport.rs`），而 relay client 拒地址正落在
                // 那个分支——真机日志里几十条全是 `error=`，判别码只有 Debug 带得出来。
                warn!(%relay_addr, error = ?e, "relay circuit listen failed");
                self.fail_relay_reservation(peer_id, "circuit listen failed");
            }
        }
    }

    /// 记一次 reservation 的**同步**失败：翻 `Failed` + 抬高重试闸门。
    ///
    /// 只给同步失败用。异步失败（reservation 被 relay 拒、listener 事后关闭）走
    /// `RelayReservationLost` + core `InfraSupervisor` 的退避，不经此处——那类失败
    /// 换个时机重试确实可能成功，而同步失败在输入不变时永远是同一个答案。
    fn fail_relay_reservation(&mut self, peer: PeerId, error: impl Into<String>) {
        let error = error.into();
        let now = Instant::now();
        let entry = self.relay_retry.entry(peer).or_insert(RelayRetry {
            attempts: 0,
            retry_after: now,
            last_error: String::new(),
        });
        entry.attempts = entry.attempts.saturating_add(1);
        entry.retry_after = now + reservation_retry_backoff(entry.attempts);
        entry.last_error = error.clone();
        self.set_relay_failed(peer, error);
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
        // 同 handle_remove_infra_peer：意图撤销后闸门记账不该留到下一次登记
        self.relay_retry.remove(&peer);

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
        let candidates = self.peer_addrs(&peer);

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

                // 这条地址刚被证明可用——刷新它的「最近提及」序号，免得日后被新涌入的
                // 地址淘汰掉。**仅出站**：入站连接的远端地址是对端的临时源端口
                //（TCP 随机高位端口），它连可拨地址都不是，进簿只会挤掉真候选。
                if endpoint.is_dialer() {
                    self.refresh_addr(peer_id, endpoint.get_remote_address());
                }

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
                let announces_lan = self
                    .config
                    .relay_server
                    .as_ref()
                    .is_some_and(|s| s.announce_private_addrs)
                    && (addr.is_private_lan() || addr.is_loopback());
                // 经账本登记而不是直接推给 Swarm —— 直接推的话它就成了「视图不知道、
                // 因而永远撤不掉」的那一类。WebTransport 的监听地址带 certhash、
                // 每 14 天变一次，撤不掉就会一直累积。
                let mut external_changed = false;
                if announces_lan && !self.lan_announced.contains(&addr) {
                    self.lan_announced.push(addr.clone());
                    external_changed = true;
                }
                self.watches
                    .addrs
                    .send_modify(|info| info.listen.push(addr));
                // 公网 IP 映射跟着监听集合走，必须在视图更新**之后**重算。
                external_changed |= self.recompute_mapped_external();
                if external_changed {
                    // 返回值可丢：监听视图已经变了，下面那句 publish 无论如何都要发。
                    let _ = self.resync_external();
                }
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
                if !self.confirmed_external.contains(&addr) {
                    self.confirmed_external.push(addr);
                    if self.resync_external() {
                        self.publish_addrs();
                    }
                }
            }
            SwarmEvent::ExternalAddrExpired { address } => {
                let addr = Addr::from_multiaddr(address);
                self.confirmed_external.retain(|a| *a != addr);
                // ⚠️ **账本里也要摘掉**：上游发这个事件之前**已经**自己
                // `remove_external_address` 了（`swarm/src/lib.rs` 的
                // `ToSwarm::ExternalAddrExpired` 分支无条件执行），Swarm 里此刻已经没有它。
                //
                // 不摘的话账本就与 Swarm 不一致，而差量是按账本算的：若这条地址同时还被
                // 宿主声明着（`declared_external`），`resync_external` 会认为「desired 有、
                // 账本也有 ⇒ 无需动作」而**跳过重新登记** —— 于是 Swarm 永久不再通告它，
                // 视图与 UI 却仍然说它可达。摘掉之后差量才会把它重新 add 回去。
                self.swarm_external.retain(|a| *a != addr);
                if self.resync_external() {
                    self.publish_addrs();
                }
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
        // LanHelper 登记过的那几条要跟着撤：上游的 `ExpiredListenAddr` **不动**
        // external 集合（`swarm/src/lib.rs` 只从 `listened_addrs` 摘），所以撤销只能由
        // 这里发起。漏了它，证书轮换后的旧 WebTransport 地址会永久留在通告集合里。
        let before = self.lan_announced.len();
        self.lan_announced
            .retain(|a| !removed.contains(a.as_multiaddr()));
        let mut external_changed = self.lan_announced.len() != before;

        let listen_changed = self.watches.addrs.send_if_modified(|info| {
            let before = info.listen.len();
            info.listen.retain(|a| !removed.contains(a.as_multiaddr()));
            info.listen.len() != before
        });
        // 同上，公网 IP 映射从更新后的 listen 视图整份重算，与 LanHelper 那份合并成
        // 一次 resync、一次 publish。
        external_changed |= self.recompute_mapped_external();
        let view_changed = external_changed && self.resync_external();
        if listen_changed || view_changed {
            self.publish_addrs();
        }
    }

    /// 取下一个地址簿逻辑时刻。**自增与使用绑在一起**，调用方不必记住「先自增」这个约定。
    fn next_addr_clock(&mut self) -> u64 {
        self.addr_clock += 1;
        self.addr_clock
    }

    /// 刷新一条**已在簿中**地址的「最近提及」序号；不在簿里则什么都不做。
    ///
    /// 与 [`record_addr`](Self::record_addr) 的区别：这里不新增。用于「这条地址刚被证明
    /// 可用」——把一条从没进过簿的临时源地址塞进去只会挤掉真候选。
    fn refresh_addr(&mut self, peer: PeerId, addr: &libp2p::Multiaddr) {
        let clock = self.next_addr_clock();
        if let Some(book) = self.address_book.get_mut(&peer)
            && let Some(entry) = book.iter_mut().find(|e| same_dial_target(&e.addr, addr))
        {
            entry.touched = clock;
        }
    }

    /// 某对端的候选地址（按进簿顺序，最新的在前）。
    ///
    /// 顺序即 libp2p 的拨号优先级，故**不**按 [`AddrEntry::touched`] 重排——那个序号只
    /// 决定淘汰谁，让它同时决定拨号顺序的话，顺序会随 mDNS 的广播节奏抖动。
    fn peer_addrs(&self, peer: &PeerId) -> Vec<libp2p::Multiaddr> {
        self.address_book
            .get(peer)
            .map(|book| book.iter().map(|e| e.addr.clone()).collect())
            .unwrap_or_default()
    }

    /// 地址进簿（去重 + 有界）+ 广播给 behaviour（kad 等各自决定是否收录）。
    ///
    /// **以本机为中转的 circuit 地址一律丢弃。** 它是四条进簿路径（`AddAddrs` /
    /// `AddInfraPeer` / `Connect` 显式候选 / mDNS）的共同下游，故过滤收在这一处。
    fn record_addr(&mut self, peer: PeerId, addr: libp2p::Multiaddr) {
        // `trace` 而非 `debug`：默认 filter 就是 `swarmdrop_net=debug`，而这条在对端反复
        // 断连重连时是每秒上万条的量级（实测一次 11 分钟的重连风暴写了 640 MB 日志，
        // 其中 99.9% 是它）。要看被丢了什么，开 `swarmdrop_net=trace`。
        if is_relayed_by(&addr, self.node_id) {
            trace!(%peer, %addr, "skip circuit address relayed by self");
            return;
        }
        let clock = self.next_addr_clock();
        let entry = self.address_book.entry(peer).or_default();
        if touch_addr(entry, addr.clone(), clock) {
            // 新的**可用基址** = 新事实：reservation 的同步失败闸门立刻解除，不必等退避
            // 走完（与 `InfraSupervisor` 在候选 `last_seen` 刷新时重置退避同构）。
            //
            // 判据是「这条能当 circuit 基址」而不是「这条进簿了」：闸门挡的两个失败
            //（`no usable relay address` / `circuit listen failed`）都由基址决定，而新进簿
            // 的若本身是 circuit 地址，`circuit_base` 对它必然仍是 `None`——它**证明不了
            // 任何新事实**，清闸只是立刻重放一次注定相同的失败：同一条 warn + 一次
            // Connecting→Failed 的 watch 抖动（三端跟着重渲染）。真机形态是 R 既被配对
            // 又当 relay、簿里只剩到它的 circuit 地址，于是 R 的中转路径每变一次抖一次。
            //
            // 仍放在去重的 `if` 内部——mDNS 秒级重报同一条地址靠上面那层挡掉，
            // 挪到 if 外面就等于没有闸门。
            if circuit_base(addr.clone(), peer).is_some() {
                self.relay_retry.remove(&peer);
            }
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
///
/// **`None` = 这条地址当不了 circuit 基址**，两种情形，libp2p 的 relay client transport
/// 都是在 `dial` / `listen_on` 里当场拒收：
///
/// 1. **它自己就含 `/p2p-circuit` 段**——再追加一层会拼出
///    `…/p2p/<A>/p2p-circuit/p2p/<B>/p2p-circuit` 这种双层地址，判别码
///    `MultipleCircuitRelayProtocolsUnsupported`。2026-08-10 真机实测刷屏几十条。
/// 2. **circuit 段之前没有任何传输段**（一条裸 `/p2p/<relay>`）——relay client 没有
///    前半段可拨，判别码 `MissingRelayAddr`。四条进簿路径里暂时还没见过这种形状
///    （`AddAddrs` / `Connect` 的显式候选不做形状校验，是个理论缺口），故属防御性拦截。
///
/// 两个判别码都落在 [`TransportError::Other`](libp2p::TransportError) 分支上、
/// **Display 写的是空串**（`core/src/transport.rs`），于是日志里只剩一条 `error=` 的空
/// 字段，谁也看不出发生了什么。所以判在这里，不留给 `listen_on` 去拒。
///
/// 与 `record_addr` 那条「以本机为中转的 circuit 地址一律丢弃」（2026-08-06，见
/// [`is_relayed_by`]）是同一族问题的两面：那条治「中转跳是自己」，本条治「拿一条中转地址
/// 当新中转的基址」。中转跳是第三方时前者按设计放行——本次的双层地址正是从那儿漏过来的。
fn circuit_base(addr: libp2p::Multiaddr, relay: PeerId) -> Option<libp2p::Multiaddr> {
    let mut has_p2p = false;
    let mut has_transport = false;
    for p in addr.iter() {
        match p {
            libp2p::multiaddr::Protocol::P2pCircuit => return None,
            libp2p::multiaddr::Protocol::P2p(_) => has_p2p = true,
            // 传输段 = 身份段与 circuit 段之外的任何一段（`/ip4`、`/dns4`、`/tcp`、
            // `/quic-v1`、`/webrtc-direct`…）。这里不逐个白名单：新传输随时会加，
            // 漏一个就是把一条本来能用的基址判死。
            _ => has_transport = true,
        }
    }
    if !has_transport {
        return None;
    }
    let base = if has_p2p {
        addr
    } else {
        addr.with(libp2p::multiaddr::Protocol::P2p(relay))
    };
    Some(base.with(libp2p::multiaddr::Protocol::P2pCircuit))
}

/// 一组候选里第一条能当 circuit 基址的地址——[`Actor::circuit_base_for`] 的挑选规则本体。
///
/// 拆成自由函数是为了让护栏测试打在**真正被调用的那份挑选逻辑**上：它只需要一组地址，
/// 而 `circuit_base_for` 要一整个 `Actor`（进而要一个 Swarm），测试够不到，于是原先在
/// 测试里另抄了一份 `find_map`——那样一来「改回 `first()`」这类回归它一条都拦不住。
fn first_circuit_base<'a>(
    addrs: impl IntoIterator<Item = &'a libp2p::Multiaddr>,
    relay: PeerId,
) -> Option<libp2p::Multiaddr> {
    addrs
        .into_iter()
        .find_map(|a| circuit_base(a.clone(), relay))
}

/// 这条地址的中转跳是不是 `node`。
///
/// 真实来路：浏览器与桌面同网直连后向桌面申请了 circuit reservation，于是它的可达地址里
/// 多出一条 `…/p2p/<桌面>/p2p-circuit/p2p/<浏览器>`，再原样广告回桌面。桌面拿它去拨，
/// 第一跳拨的就是自己——实测报 `Dial error: Unexpected peer ID`，offer 随之失败。
///
/// 这类地址**永远拨不通，也永远不需要拨**：本机能当对端的中转，前提就是两者之间已经有
/// 一条连接。留着它只会挤掉同批候选里真正可用的那些。
///
/// 判据落在中转跳而非末位：末位是自己时 libp2p 自己就拒（`DialError::LocalPeerId`），
/// 漏的正是中间这一跳。
fn is_relayed_by(addr: &libp2p::Multiaddr, node: NodeId) -> bool {
    Addr::from_multiaddr(addr.clone()).relay_node_id() == Some(node)
}

/// 两条地址是不是同一个拨号目标 —— 忽略末位的 `/p2p/<id>` 差异。
///
/// # 为什么需要它
///
/// `Swarm::dial` 会把每条候选地址做一次 `with_p2p(peer)` 之后才交给 transport，而
/// `ConnectedPoint::Dialer{ address }` 回报的正是**加过 `/p2p/` 的那一份**。地址簿里的
/// 条目却未必带这个后缀：mDNS 与硬编码 bootstrap 清单带，而 **DHT presence record 来的
/// 不带**（那是对端的 `dialable()`，libp2p 的监听地址天然没有 `/p2p/`）。
///
/// 直接 `==` 比较的后果很隐蔽：跨网对端经 presence 发现、拨通，这条「刚被证明可用」的
/// 地址**永远拿不到刷新**，于是恰恰是它先被地址簿上限淘汰掉 —— 而那正是
/// [`touch_addr`] 的 LRU 想保护的东西。
fn same_dial_target(a: &libp2p::Multiaddr, b: &libp2p::Multiaddr) -> bool {
    a == b || strip_trailing_p2p(a) == strip_trailing_p2p(b)
}

/// 去掉末位的 `/p2p/<id>` 段（没有就原样返回）。
///
/// **只去末位**：circuit 地址中间那个 `/p2p/<relay>` 是中转身份，去掉它会把两条经不同
/// relay 的地址判成同一条。
fn strip_trailing_p2p(addr: &libp2p::Multiaddr) -> libp2p::Multiaddr {
    let mut out = addr.clone();
    if matches!(out.iter().last(), Some(libp2p::multiaddr::Protocol::P2p(_))) {
        out.pop();
    }
    out
}

/// 地址簿条目：地址 + 最近一次被「提及」的逻辑序号。
///
/// **序号是计数器而不是时间戳**，因为这里要回答的只是「谁更久没被提及」这个相对问题，
/// 而绝对时间在本内核里恰恰是最不该依赖的东西：wasm target 下没有可靠的单调时钟
/// （kad 的 `Instant` 就为此在两个 target 上分叉过）。计数器在两处都是同一份代码。
#[derive(Debug, Clone)]
struct AddrEntry {
    addr: libp2p::Multiaddr,
    touched: u64,
}

/// 新地址进簿，或刷新已有地址的「最近提及」序号。返回是否**新增**（而非刷新）。
///
/// 三条判据：
///
/// - **物理顺序 = 进簿顺序，最新的排最前。** 候选顺序决定 libp2p 的拨号优先级，
///   对端刚换的 IP、刚轮换出的 certhash 地址比簿里躺了很久的那条更可能拨得通。
/// - **重报只刷新序号、不挪位置。** mDNS 秒级重报同一条，挪位置会让拨号顺序随广播
///   节奏抖动，而重报并不代表这条比别的更该先试。
/// - **超限时淘汰序号最小的，而不是排最后的。** 这两者的区别就是这个函数的全部价值：
///   一条一直可用的公网地址进簿最早、物理上排最后，但只要对端还在 identify / DHT 里
///   持续上报它，它的序号就一直是新的 —— 被淘汰的于是是那些「既不新、也再没人提起」
///   的死地址。按物理位置淘汰的话，恰恰是那条唯一还能用的地址被新涌入的私网地址挤掉。
fn touch_addr(book: &mut Vec<AddrEntry>, addr: libp2p::Multiaddr, clock: u64) -> bool {
    if let Some(entry) = book.iter_mut().find(|e| e.addr == addr) {
        entry.touched = clock;
        return false;
    }

    book.insert(
        0,
        AddrEntry {
            addr,
            touched: clock,
        },
    );

    if book.len() > MAX_ADDRS_PER_PEER {
        // 淘汰最久没被提及的那条。簿最大 32 条，线性扫比维护堆简单得多。
        if let Some(pos) = book
            .iter()
            .enumerate()
            .min_by_key(|(_, e)| e.touched)
            .map(|(i, _)| i)
        {
            book.remove(pos);
        }
    }
    true
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

/// 从对端自报的地址里挑出 LAN 升级候选：**严格优于 `current` 的那一档里最好的一档**。
///
/// 两条筛选各有各的理由，缺一条都会退回原来的毛病：
///
/// - **只要严格更优的档**：等于现状或更差的地址拨了也白拨（成功了也不算升级，还占一条连接）。
/// - **只取其中最好的一档**：libp2p 并发拨号是延迟竞速，同时发出 webrtc-direct 与
///   WebTransport，赢的多半是前者；而升级成功后就不再有第二次机会。**层内竞速、层间有序**
///   这条规则就落在这一行。
///
/// 档内仍然**每种传输各留几个**（`LAN_UPGRADE_MAX_PER_TRANSPORT`）而不是笼统取前 N 个——
/// 同一档里 IPv4/IPv6、多网卡会挤掉彼此，理由见那个常量。
fn lan_candidates(addrs: &[libp2p::Multiaddr], current: DialTier) -> Vec<libp2p::Multiaddr> {
    lan_candidates_by(addrs, current, crate::transport::can_dial)
}

/// [`lan_candidates`] 的纯逻辑内核，「本端拨得动吗」由参数注入。
///
/// 注入而不是直接调 [`crate::transport::can_dial`]，是为了让**浏览器那一格能在 native 上测**
/// ——本 crate 的测试只跑 native，而那一格恰恰是唯一会出错的一格（见
/// `dialable_filter_runs_before_tier_selection`）。
fn lan_candidates_by(
    addrs: &[libp2p::Multiaddr],
    current: DialTier,
    can_dial: impl Fn(&Addr) -> bool,
) -> Vec<libp2p::Multiaddr> {
    let better: Vec<(DialTier, &libp2p::Multiaddr)> = addrs
        .iter()
        .map(|a| (Addr::from_multiaddr(a.clone()), a))
        // **筛「拨得动」必须排在挑档之前。** 浏览器最快的一档恰好是它唯一拨不动的
        // （对端自报的 `/tcp` / `/quic-v1`），先挑档再筛就会永远挑中拨不动的那条。
        .filter(|(addr, _)| is_lan_candidate(addr) && can_dial(addr))
        .map(|(addr, raw)| (addr.dial_tier(), raw))
        .filter(|(tier, _)| *tier < current)
        .collect();

    let Some(best) = better.iter().map(|(tier, _)| *tier).min() else {
        return Vec::new();
    };

    let mut taken: HashMap<Option<TransportKind>, usize> = HashMap::new();
    better
        .into_iter()
        .filter(|(tier, _)| *tier == best)
        .filter(|(_, a)| {
            let slot = taken
                .entry(Addr::from_multiaddr((*a).clone()).transport())
                .or_default();
            *slot += 1;
            *slot <= LAN_UPGRADE_MAX_PER_TRANSPORT
        })
        .map(|(_, a)| a.clone())
        .collect()
}

/// LAN 升级的候选判据：私网可达，且不是我们正想摆脱的那类 circuit 地址。
///
/// 排除 circuit 那半边不是多余的——LAN helper（局域网内的中继）自己就监听在
/// 私网地址上，它派发的 circuit 地址前半段同样 `is_private_lan()`，不排除就会
/// 把「换一条中继」当成「升级为直连」。
fn is_lan_candidate(addr: &Addr) -> bool {
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
    /// 真实 certhash：占位串不是合法 multibase multihash，地址会解析失败。
    const H1: &str = "uEiBuBPteUjlXiXM9izTtEdpg3C0QHFZ0A2m6aSjsbv2oeA";
    const H2: &str = "uEiDSOtFQBoepe-LRH2mZPMLHGoMcxnmaM8a02_72my1v9Q";

    fn multiaddrs(items: &[&str]) -> Vec<libp2p::Multiaddr> {
        items
            .iter()
            .map(|s| s.parse().expect("valid multiaddr"))
            .collect()
    }

    fn transports(addrs: &[libp2p::Multiaddr]) -> Vec<Option<TransportKind>> {
        addrs
            .iter()
            .map(|a| Addr::from_multiaddr(a.clone()).transport())
            .collect()
    }

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

    /// 以本机为中转的 circuit 地址不进地址簿。
    ///
    /// 它红了意味着桌面会拿「经桌面自己中转到浏览器」这条地址去拨号——第一跳拨的就是自己，
    /// 实测 `Dial error: Unexpected peer ID`，而它还占着同批候选里的位置。
    ///
    /// 三条反例各钉一种误伤：中转是别人的照收（这是中继的正常形态）、末位是本机的不归它管
    /// （libp2p 自己拒）、以及**本机的直连地址照收**——`relay_node_id()` 只在 circuit 段
    /// 之前取 `/p2p/`，若误写成「地址里出现过本机 id 就丢」，`/p2p/<本机>` 结尾的普通地址
    /// 会一起被丢掉。
    #[test]
    fn self_relayed_circuit_addr_is_rejected() {
        let me = NodeId::from_peer_id(RELAY.parse::<PeerId>().expect("valid peer id"));
        let by_me = |s: &str| is_relayed_by(&s.parse().expect("valid multiaddr"), me);

        assert!(
            by_me(&format!(
                "/ip4/192.168.1.9/tcp/4001/p2p/{RELAY}/p2p-circuit/p2p/{PEER}"
            )),
            "中转跳是本机：拨它等于拨自己"
        );
        assert!(
            by_me(&format!(
                "/ip4/192.168.1.9/udp/4001/webrtc-direct/p2p/{RELAY}/p2p-circuit/p2p/{PEER}"
            )),
            "浏览器场景的真实形态：前半段是 webrtc-direct 到本机"
        );

        assert!(
            !by_me(&format!(
                "/ip4/1.2.3.4/tcp/4001/p2p/{PEER}/p2p-circuit/p2p/{RELAY}"
            )),
            "中转是别人、目标才是本机——这是正常的中继地址"
        );
        assert!(
            !by_me(&format!("/ip4/192.168.1.9/tcp/4001/p2p/{RELAY}")),
            "本机的直连地址不含 circuit 段，不能一起丢"
        );
        assert!(
            !by_me("/ip4/192.168.1.9/tcp/4001"),
            "无 /p2p/ 段的裸地址同上"
        );
    }

    /// circuit 基址不得从一条 circuit 地址上再长一层。
    ///
    /// 它红了意味着 `listen_on` 会收到 `…/p2p/<A>/p2p-circuit/p2p/<B>/p2p-circuit`，
    /// libp2p 的 relay client 以 `MultipleCircuitRelayProtocolsUnsupported` 当场拒收——
    /// 而那个判别码落在 `TransportError::Other` 上、Display 是空串，日志里只剩
    /// `error=`。2026-08-10 真机上它以秒级频率刷了几十条，谁也看不出发生了什么。
    #[test]
    fn circuit_base_never_nests() {
        let relay = RELAY.parse::<PeerId>().expect("valid peer id");
        let base = |s: &str| circuit_base(s.parse().expect("valid multiaddr"), relay);

        // 真机日志里那条地址的形状：经 A 中转到 B，B 又被当成新的 relay
        assert_eq!(
            base(&format!(
                "/ip4/192.168.50.105/udp/4001/quic-v1/p2p/{PEER}/p2p-circuit/p2p/{RELAY}"
            )),
            None,
            "已含 circuit 段的地址当不了基址"
        );
        assert_eq!(
            base(&format!("/ip4/1.2.3.4/tcp/4001/p2p/{RELAY}/p2p-circuit")),
            None,
            "以 /p2p-circuit 结尾的同理——再追加就是两段"
        );

        // circuit 段之前没有传输段：relay client 无从拨起，`MissingRelayAddr`。
        // 这两格是防御性的（暂无进簿路径产出这种形状），但拒的判据与上面同源。
        assert_eq!(
            base(&format!("/p2p/{RELAY}")),
            None,
            "裸 /p2p/<relay> 没有传输段，当不了基址"
        );
        assert_eq!(
            circuit_base(libp2p::Multiaddr::empty(), relay),
            None,
            "空地址同理——`circuit_addr_for` 的退化展示值因此自己拼，不走这里"
        );

        // 正常地址仍要正常拼
        let direct = base(&format!("/ip4/1.2.3.4/tcp/4001/p2p/{RELAY}"))
            .expect("直连地址必须能当基址")
            .to_string();
        assert_eq!(
            direct,
            format!("/ip4/1.2.3.4/tcp/4001/p2p/{RELAY}/p2p-circuit")
        );

        // 不带 /p2p/ 的地址补上 relay 身份后再接 circuit
        let bare = base("/ip4/1.2.3.4/tcp/4001")
            .expect("裸地址必须能当基址")
            .to_string();
        assert_eq!(
            bare,
            format!("/ip4/1.2.3.4/tcp/4001/p2p/{RELAY}/p2p-circuit")
        );

        // 结果永远只有一段 circuit——这是本测试要钉的不变量
        for out in [direct, bare] {
            assert_eq!(out.matches("/p2p-circuit").count(), 1, "{out}");
        }
    }

    /// 地址簿里混着 circuit 地址时，基址要跳过它而不是撞上它。
    ///
    /// `first()` 版本的失败模式极隐蔽：地址簿的顺序取决于哪条先被 identify / mDNS /
    /// DHT 报上来，于是同一份代码在某些网络下正常、在另一些下 reservation 永远建不起来。
    #[test]
    fn circuit_base_skips_circuit_entries_in_address_book() {
        let relay = RELAY.parse::<PeerId>().expect("valid peer id");
        let book: Vec<libp2p::Multiaddr> = [
            // 排第一的是经第三方中转到该 relay 的地址（`record_addr` 按设计放行：
            // 中转跳不是本机）
            format!("/ip4/192.168.50.105/udp/4001/quic-v1/p2p/{PEER}/p2p-circuit/p2p/{RELAY}"),
            format!("/ip4/1.2.3.4/tcp/4001/p2p/{RELAY}"),
        ]
        .iter()
        .map(|s| s.parse().expect("valid multiaddr"))
        .collect();

        let picked = first_circuit_base(&book, relay).expect("应挑出第二条");
        assert_eq!(
            picked.to_string(),
            format!("/ip4/1.2.3.4/tcp/4001/p2p/{RELAY}/p2p-circuit")
        );
    }

    fn maddr(n: usize) -> libp2p::Multiaddr {
        format!("/ip4/10.0.0.{}/tcp/4001", n % 256)
            .parse()
            .expect("valid multiaddr")
    }

    fn addrs_of(book: &[AddrEntry]) -> Vec<libp2p::Multiaddr> {
        book.iter().map(|e| e.addr.clone()).collect()
    }

    /// 地址簿必须有界：只去重不淘汰的话，长期在线的对端会累积几十条早已拨不通的地址，
    /// 占满 libp2p 默认 8 条的并发拨号预算。
    #[test]
    fn address_book_is_bounded() {
        let mut book = Vec::new();
        for n in 0..(MAX_ADDRS_PER_PEER + 10) {
            touch_addr(&mut book, maddr(n), n as u64);
        }

        assert_eq!(book.len(), MAX_ADDRS_PER_PEER);
        // 最新的排最前 —— 物理顺序即拨号优先级。
        assert_eq!(book[0].addr, maddr(MAX_ADDRS_PER_PEER + 9));
        assert!(
            !addrs_of(&book).contains(&maddr(0)),
            "最久没被提及的应被淘汰"
        );
    }

    /// 重复上报同一条地址不增长、不挪位置，但**要刷新序号** —— mDNS 是秒级重报的，
    /// 而「还在被重报」正是这条地址仍然活着的证据。
    #[test]
    fn re_recording_a_known_address_refreshes_without_reordering() {
        let mut book = Vec::new();
        touch_addr(&mut book, maddr(1), 1);
        touch_addr(&mut book, maddr(2), 2);
        let order_before = addrs_of(&book);

        assert!(!touch_addr(&mut book, maddr(1), 99), "已知地址不算新增");

        assert_eq!(addrs_of(&book), order_before, "重报不得改变拨号顺序");
        let refreshed = book.iter().find(|e| e.addr == maddr(1)).expect("仍在簿中");
        assert_eq!(
            refreshed.touched, 99,
            "重报必须刷新序号，否则它会被当成死地址淘汰"
        );
    }

    /// **本次修复的核心护栏。**
    ///
    /// 场景：对端在局域网里频繁换 IP（每次 mDNS 报一条新地址），而真正一直可用的是那条
    /// 最早进簿、物理上排在最后的公网地址 —— 它仍在被 identify 持续上报。
    ///
    /// 按物理位置淘汰的话，被挤掉的恰恰是它，对端从此再也拨不通；按「最近被提及」淘汰，
    /// 被挤掉的才是那些既不新、也再没人提起的死地址。
    #[test]
    fn still_advertised_address_survives_a_flood_of_new_ones() {
        let live = maddr(200);
        let mut clock = 0u64;
        let mut tick = || {
            clock += 1;
            clock
        };

        let mut book = Vec::new();
        touch_addr(&mut book, live.clone(), tick());

        // 用新地址把簿填满，`live` 因此被推到**物理最末位** —— 这一步是本测试的关键：
        // 若让它留在前面，按物理位置淘汰的实现也能让它存活，测试就测不出区别了。
        for n in 0..(MAX_ADDRS_PER_PEER - 1) {
            touch_addr(&mut book, maddr(n), tick());
        }
        assert_eq!(book.len(), MAX_ADDRS_PER_PEER);
        assert_eq!(book[MAX_ADDRS_PER_PEER - 1].addr, live, "live 应在最末位");

        // 对端的 identify 仍在上报它 —— 只刷新序号，不改变物理位置。
        touch_addr(&mut book, live.clone(), tick());

        // 随后又涌入一批新地址。
        for n in 100..110 {
            touch_addr(&mut book, maddr(n), tick());
        }

        assert!(
            addrs_of(&book).contains(&live),
            "仍在被上报的地址不该被淘汰（按物理位置淘汰就会挤掉它）：{:?}",
            addrs_of(&book)
        );
        assert_eq!(book.len(), MAX_ADDRS_PER_PEER);
    }

    /// 反面：不再被上报的地址**应该**被淘汰掉，否则上限就形同虚设。
    #[test]
    fn silent_address_is_evicted_under_pressure() {
        let stale = maddr(200);
        let mut book = Vec::new();
        touch_addr(&mut book, stale.clone(), 0);

        for n in 0..(MAX_ADDRS_PER_PEER * 2) {
            touch_addr(&mut book, maddr(n), (n + 1) as u64);
        }

        assert!(
            !addrs_of(&book).contains(&stale),
            "再没被提及过的地址应被淘汰"
        );
    }

    /// **护栏：`/p2p/` 后缀的有无不影响「是不是同一个拨号目标」。**
    ///
    /// `Swarm::dial` 会给候选地址补 `with_p2p(peer)` 再交给 transport，而回报的
    /// `ConnectedPoint::Dialer{address}` 是补过的那份。地址簿里 DHT presence 来的条目
    /// 却不带后缀 —— 直接 `==` 比的话，跨网对端那条刚拨通的地址永远刷新不到序号，
    /// 于是恰恰是它先被上限淘汰（正是 LRU 想保护的东西）。
    #[test]
    fn dial_target_matching_ignores_trailing_p2p() {
        let bare: libp2p::Multiaddr = "/ip4/1.2.3.4/udp/4001/quic-v1".parse().unwrap();
        let with_p2p: libp2p::Multiaddr = format!("/ip4/1.2.3.4/udp/4001/quic-v1/p2p/{PEER}")
            .parse()
            .unwrap();

        assert!(same_dial_target(&bare, &with_p2p));
        assert!(same_dial_target(&with_p2p, &bare));
        assert!(same_dial_target(&bare, &bare));
    }

    /// 反面：**只去末位**的 `/p2p/`。circuit 地址中间那个是中转身份，两条经不同 relay
    /// 的地址不是同一个拨号目标。
    #[test]
    fn dial_target_matching_keeps_relay_identity() {
        let via_a: libp2p::Multiaddr = format!("/ip4/1.2.3.4/tcp/4001/p2p/{RELAY}/p2p-circuit")
            .parse()
            .unwrap();
        let via_b: libp2p::Multiaddr = format!("/ip4/1.2.3.4/tcp/4001/p2p/{PEER}/p2p-circuit")
            .parse()
            .unwrap();

        assert!(!same_dial_target(&via_a, &via_b));
    }

    /// 不同端口仍是不同目标 —— 别把它写成「只比 IP」。
    #[test]
    fn dial_target_matching_still_distinguishes_addresses() {
        let a: libp2p::Multiaddr = "/ip4/1.2.3.4/udp/4001/quic-v1".parse().unwrap();
        let b: libp2p::Multiaddr = "/ip4/1.2.3.4/udp/4002/quic-v1".parse().unwrap();

        assert!(!same_dial_target(&a, &b));
    }

    /// 闸门档位随连续失败递增并封顶，且与 core `InfraSupervisor::rebuild_backoff` 同一套。
    ///
    /// 它退化成常数（或漏了封顶）就回到本次要修的那个形态：注定失败的地址被秒级重放。
    #[test]
    fn reservation_backoff_escalates_and_caps() {
        let steps: Vec<u64> = (0..7)
            .map(|n| reservation_retry_backoff(n).as_secs())
            .collect();
        assert_eq!(steps, vec![2, 2, 5, 10, 30, 75, 75]);
        assert!(
            steps.windows(2).all(|w| w[1] >= w[0]),
            "退避不得回落，否则失败越久重试越密"
        );
    }

    /// LAN 升级只认「私网且非 circuit」。少了后半个条件，LAN helper 派发的
    /// circuit 地址（前半段也是私网）会被当成直连候选——那只是换了条中继。
    #[test]
    fn lan_upgrade_candidates_exclude_circuit_and_unreachable() {
        let lan =
            |s: &str| is_lan_candidate(&Addr::from_multiaddr(s.parse().expect("valid multiaddr")));

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

    /// **升级只拨「本端拨得动的、严格更好的那一档」，且只拨那一档。**
    ///
    /// 层内竞速没问题（同档差别不大），层间必须有序：libp2p 并发拨号是**延迟**竞速，
    /// webrtc-direct 与 WebTransport 一起发出去赢的多半是前者，而升级成功后
    /// `current` 就不再劣于任何候选，**再没有第二次机会**。
    #[test]
    fn lan_candidates_take_only_the_best_dialable_tier() {
        let addrs = multiaddrs(&[
            "/ip4/192.168.1.5/udp/54323/webrtc-direct",
            &format!("/ip4/192.168.1.5/udp/54324/quic-v1/webtransport/certhash/{H1}/certhash/{H2}"),
        ]);

        let picked = lan_candidates(&addrs, DialTier::Relayed);
        let kinds = transports(&picked);

        assert_eq!(
            kinds,
            vec![Some(TransportKind::Webtransport)],
            "两档同时可拨时只能拨快的那一档，否则竞速会落到 webrtc-direct：{picked:?}"
        );
    }

    /// **浏览器那一格全靠这条。** 浏览器拨不了裸 TCP/QUIC——而那恰好是对端自报里
    /// 最快的一档。先挑档再筛「拨不拨得动」，浏览器会永远挑中拨不动的那条：
    /// 拨号立刻失败 → 在途标记清掉 → 5 分钟后 identify 再来一轮挑同一条，
    /// **永远升不上去**，且每一步看起来都在正常工作。
    #[test]
    fn dialable_filter_runs_before_tier_selection() {
        // 对端是原生端的典型自报：最快的一档（TCP/QUIC）浏览器一条都拨不动，
        // 而它同时还给了 webrtc-direct。
        let addrs = multiaddrs(&[
            "/ip4/192.168.1.5/tcp/54321",
            "/ip4/192.168.1.5/udp/54322/quic-v1",
            "/ip4/192.168.1.5/udp/54323/webrtc-direct",
        ]);
        // 浏览器：拨不了 TCP/QUIC。
        let browser = |a: &Addr| {
            !matches!(
                a.transport(),
                Some(TransportKind::Tcp) | Some(TransportKind::Quic)
            )
        };

        let picked = lan_candidates_by(&addrs, DialTier::Relayed, browser);
        assert_eq!(
            transports(&picked),
            vec![Some(TransportKind::WebrtcDirect)],
            "浏览器只能拿到它拨得动的那一档；挑中 TCP/QUIC 等于永远升不上去：{picked:?}"
        );

        // 原生端在同一份地址上应当挑到更快的那一档。
        let native = |_: &Addr| true;
        let picked = lan_candidates_by(&addrs, DialTier::Relayed, native);
        assert!(
            !transports(&picked).contains(&Some(TransportKind::WebrtcDirect)),
            "原生端拨得动 TCP/QUIC，就不该退到 webrtc-direct：{picked:?}"
        );
    }

    /// 已经在最好的一档上时不再拨——拨了也不是升级，只会白占一条连接。
    #[test]
    fn lan_candidates_never_sidegrade_or_downgrade() {
        let addrs = multiaddrs(&["/ip4/192.168.1.5/udp/54323/webrtc-direct"]);

        assert!(
            lan_candidates(&addrs, DialTier::DirectSlow).is_empty(),
            "同档不算升级"
        );
        assert!(
            lan_candidates(&addrs, DialTier::DirectFast).is_empty(),
            "更差的一档更不该拨"
        );
        assert!(
            !lan_candidates(&addrs, DialTier::Relayed).is_empty(),
            "中继 → 直连仍然要拨"
        );
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

        assert_eq!(
            lan_candidates(&addrs, DialTier::Relayed).len(),
            LAN_UPGRADE_MAX_PER_TRANSPORT
        );
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
