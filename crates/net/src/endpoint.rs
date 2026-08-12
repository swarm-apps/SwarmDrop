//! [`Endpoint`]：内核门面（`Arc<Inner>` newtype，Clone 廉价）。
//!
//! 所有可变状态在后台 actor 里；Endpoint 只持命令通道、流控制柄与
//! watch 读端。用户永不接触事件循环（对比旧栈把 `EventReceiver` 直接
//! 交给上层消费）。
//!
//! identify 的字段里，**只有 `agent_version` 可以运行时修改**
//! （[`Endpoint::set_agent_version`]）。`protocol_version` 刻意不开：它是
//! 兼容性判别码（本仓用 `/swarmdrop/2.0.0`，上层拿它筛 SwarmDrop 节点），
//! 运行期翻转会让对端的兼容判断在连接中途变卦，语义上是另一回事。

pub mod builder;
pub mod presets;

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use libp2p::StreamProtocol;
use swarmdrop_net_base::{Addr, NatStatus, NodeAddr, NodeId, PathKind, ProtocolId, TransportKind};
use tokio::sync::{mpsc, oneshot, watch};
use tokio_util::sync::CancellationToken;

pub use builder::{BindError, Builder};

use crate::actor::{ActorMessage, subscriber_channel};
use crate::error::{ConnectError, Error, OpenError};
use crate::event::Events;
use crate::stream::{Direction, P2pStream, StreamRegistry};
use crate::watch::Watcher;

/// 与某对端的连接快照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnInfo {
    /// 路径分类（Local / Direct / Relayed）。
    pub path: PathKind,
    /// 连接的远端地址。
    pub addr: Addr,
    /// 最近一次 ping 的 RTT 采样（连接建立初期为 `None`）。
    pub rtt: Option<Duration>,
}

/// 本机地址视图。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AddrsInfo {
    /// 正在监听的本地地址。
    pub listen: Vec<Addr>,
    /// 已确认的外部可达地址（AutoNAT / identify 观测）。
    pub external: Vec<Addr>,
}

impl AddrsInfo {
    /// 可供对端拨号的地址（监听 ∪ 外部确认，去重）。
    ///
    /// 分享码 record / 在线宣告都靠它给对端一份可拨地址集。
    pub fn dialable(&self) -> Vec<Addr> {
        crate::addrset::union_preserving_order(&self.listen, &self.external)
    }
}

/// relay reservation 状态。
///
/// 状态机对失败诚实：拨号失败（全部候选地址耗尽）与 reservation 失效都
/// 翻转到 [`Failed`](RelayState::Failed)，观察者可区分「正在连接」与
/// 「连接失败」。机制层只报告可自证的事实——**不携带重试轮数**：轮数的
/// 语义由上层退避策略定义（core 的 InfraSupervisor 是重试记账的唯一主人，
/// 诊断经其 tracing 日志输出），内核不自行重试，每轮重试经
/// `add_infrastructure_peer` 幂等触发。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayState {
    /// 正在建立（拨号或等待 relay 接受）。
    Connecting,
    /// reservation 已被接受（可经该 relay 被动接收连接）。
    Active {
        /// 本机经该 relay 的完整可达地址（`<relay>/p2p-circuit/p2p/<本机>`），
        /// 由内核拼装下发——调用方不自行拼接（单一事实源）。
        circuit_addr: Addr,
    },
    /// 尝试失败（拨号候选地址耗尽 / reservation 被拒或失效）。
    Failed {
        /// 末次错误描述。
        last_error: String,
    },
}

/// 基础设施节点角色（`add_infrastructure_peer`）。
#[derive(Debug, Clone, Copy, Default)]
pub struct InfraRoles {
    /// 作为中继：请求 reservation（断线后经 identify 幂等重建）。
    pub relay: bool,
    /// 作为 DHT server：加进 kad 路由表（bootstrap 依赖至少一个）。
    pub kad_server: bool,
}

impl InfraRoles {
    /// bootstrap 节点的典型角色：relay + kad server。
    pub fn bootstrap() -> Self {
        Self {
            relay: true,
            kad_server: true,
        }
    }
}

/// 关停宽限：actor 收到 Shutdown 后的最长等待。
const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

/// 网络内核门面。Clone 只 clone 一个 Arc，放心分发给各协议服务。
#[derive(Clone)]
pub struct Endpoint {
    inner: Arc<Inner>,
}

pub(crate) struct Inner {
    node_id: NodeId,
    actor_tx: mpsc::Sender<ActorMessage>,
    /// 开流快路径：不经 actor 直达 muxer（等价 iroh 的 ReadOnlyMap 快路径）。
    control: libp2p_stream::Control,
    registry: StreamRegistry,
    watch_addrs: watch::Receiver<AddrsInfo>,
    watch_nat: watch::Receiver<NatStatus>,
    watch_conns: watch::Receiver<BTreeMap<NodeId, ConnInfo>>,
    watch_relays: watch::Receiver<BTreeMap<NodeId, RelayState>>,
    /// Builder 启用 DHT 时为 Some。
    dht: Option<crate::dht::Dht>,
    /// 本端点实际装配的可拨传输（bind 时按 target + 配置算一次，此后不变）。
    supported_transports: Box<[TransportKind]>,
    connect_timeout: Duration,
    next_connect_request_id: AtomicU64,
    closed: CancellationToken,
    actor_handle: Mutex<Option<n0_future::task::JoinHandle<()>>>,
}

impl std::fmt::Debug for Endpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Endpoint")
            .field("node_id", &self.inner.node_id)
            .finish_non_exhaustive()
    }
}

impl Endpoint {
    /// 新建配置 Builder。
    pub fn builder() -> Builder {
        Builder::default()
    }

    /// 本节点身份。
    pub fn node_id(&self) -> NodeId {
        self.inner.node_id
    }

    /// 本端点实际装配的可拨传输种类。
    ///
    /// 用途是**提交前同步校验**：用户粘一条引导节点地址时，据此判断本端拨不拨得动
    /// （浏览器收到 `/tcp/`、桌面收到只有对端才有的传输，都应当场拒掉而不是让它静静地
    /// 永远连不上）。它是内核事实，不是部署配置——地址清单归各端的 bootstrap-nodes 表，
    /// 两件事不要合并。
    ///
    /// circuit 地址取的是外层中继段的传输，正是本机要拨的那一跳。
    pub fn supported_transports(&self) -> &[TransportKind] {
        &self.inner.supported_transports
    }

    // ── 连接管理 ──

    /// 连接到对端。已连接时立即返回当前最优路径快照。
    ///
    /// `NodeAddr.addrs` 为空表示只知道身份——依靠地址簿既有地址
    /// （AddressLookup 解析管线在 M2 接入此路径）。
    pub async fn connect(&self, addr: impl Into<NodeAddr>) -> Result<ConnInfo, ConnectError> {
        self.connect_with_timeout(addr, self.inner.connect_timeout)
            .await
    }

    /// 在指定时限内连接到对端。
    ///
    /// 超时不仅会结束调用方等待，还会通知 actor 清理已关闭的等待者；若没有
    /// 其他调用者或基础设施角色使用该 peer，会中止底层 pending dial。
    pub async fn connect_with_timeout(
        &self,
        addr: impl Into<NodeAddr>,
        timeout: Duration,
    ) -> Result<ConnInfo, ConnectError> {
        let addr = addr.into();
        let node = addr.id;
        let request_id = self
            .inner
            .next_connect_request_id
            .fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.inner
            .actor_tx
            .send(ActorMessage::Connect {
                addr,
                request_id,
                reply: tx,
            })
            .await
            .map_err(|_| ConnectError::Closed)?;
        match n0_future::time::timeout(timeout, rx).await {
            Err(_) => {
                // request_id 精确对应本次调用，不依赖 oneshot receiver 的销毁时序，
                // 不会误删同一 peer 的并发等待者。
                let _ = self
                    .inner
                    .actor_tx
                    .send(ActorMessage::CancelConnect { node, request_id })
                    .await;
                Err(ConnectError::Timeout)
            }
            Ok(Err(_)) => Err(ConnectError::Closed),
            Ok(Ok(result)) => result,
        }
    }

    /// 断开与对端的全部连接。
    pub async fn disconnect(&self, node: NodeId) -> Result<(), Error> {
        self.request(|reply| ActorMessage::Disconnect { node, reply })
            .await
    }

    /// 是否与对端有活跃连接（同步读 watch，不经 actor）。
    pub fn is_connected(&self, node: NodeId) -> bool {
        self.inner.watch_conns.borrow().contains_key(&node)
    }

    /// 向地址簿注入对端候选地址（配对码 record、用户配置等）。
    pub async fn add_addrs(&self, node: NodeId, addrs: Vec<Addr>) -> Result<(), Error> {
        self.request(|reply| ActorMessage::AddAddrs { node, addrs, reply })
            .await
    }

    /// 声明本节点当前的外部可达地址（**整份替换，幂等**）。
    ///
    /// WebRTC Direct 与 WebTransport 的 `certhash` 只有 listener 实际启动后才出现在地址里，
    /// 公网节点观察到之后经此方法登记，使 reservation 与 identify 都能公布完整可拨地址。
    ///
    /// # 为什么是「声明整份」而不是 add / remove 一对
    ///
    /// 带 `certhash` 的地址会随证书轮换而失效，所以登记方**必须**能撤销。而命令式的
    /// add/remove 会把一份易错的记账纪律推给每个调用方：要自己维护「已登记集合」、要在
    /// 部分失败时只回滚失败的那些、还要保证「先记账后调用」这个顺序不写反（写反的后果是
    /// 一次瞬时失败让某条地址**永久**不再重试，而日志只有一行 warn）。
    ///
    /// 声明式把这些全部消掉：调用方每轮把「现在应该通告什么」整份发过来，重试就是把同一
    /// 份声明再发一次。差量由内核算，只算一次、只有一份实现。
    ///
    /// 不撤销失效地址的后果分两级，第二级是静默的：拨号预算（默认 8 条并发）会被死地址
    /// 占满，使连接成功率随运行时长下降；而 identify payload 的 4096 字节上限**只在解码端
    /// 检查**，超限时不是本机报错，是**每个对端**都解不出这条 identify。
    ///
    /// # 与自动发现的关系
    ///
    /// 本方法只替换**宿主声明的**那一份。AutoNAT / identify 观测确认的地址由内核单独持有，
    /// 不受影响——两者的并集才是对外通告的集合。
    pub async fn set_external_addrs(&self, addrs: Vec<Addr>) -> Result<(), Error> {
        self.request(|reply| ActorMessage::SetExternalAddrs { addrs, reply })
            .await
    }

    /// 运行期改写 identify 的 agent_version（构造期初值见 [`Builder::agent_version`]）。
    ///
    /// 返回时新值已经生效，并已向**所有已连接对端**主动 push 一次 identify，
    /// 对端在一个 RTT 内即可看到新值——不必重启节点、不断开任何连接。
    ///
    /// 未连接的对端**不排队补推**：新连接建立时 handler 直接取当前值，天然是新的。
    ///
    /// 值未变时是 no-op，不产生任何网络行为。
    pub async fn set_agent_version(&self, agent_version: String) -> Result<(), Error> {
        self.request(|reply| ActorMessage::SetAgentVersion {
            agent_version,
            reply,
        })
        .await
    }

    /// 保活白名单：白名单内对端的连接豁免空闲回收（已配对设备用）。
    pub async fn set_keep_alive(&self, node: NodeId, enabled: bool) -> Result<(), Error> {
        self.request(|reply| ActorMessage::SetKeepAlive {
            node,
            enabled,
            reply,
        })
        .await
    }

    // ── 基础设施 ──

    /// 经指定 relay 请求 circuit reservation（幂等）。接受结果异步到达：
    /// 观察 `watch_relays` 或 `RelayReservationAccepted` 事件。
    ///
    /// 浏览器被动接收连接的唯一入口：reservation 建立后对端可拨
    /// `<relay>/p2p-circuit/p2p/<本机>`。
    ///
    /// 语义即「注册一个只承担 relay 角色的基础设施节点」——是
    /// [`add_infrastructure_peer`](Self::add_infrastructure_peer) 的糖，actor 侧单一路径。
    pub async fn ensure_relay_reservation(&self, relay: impl Into<NodeAddr>) -> Result<(), Error> {
        self.add_infrastructure_peer(
            relay,
            InfraRoles {
                relay: true,
                kad_server: false,
            },
        )
        .await
    }

    /// 撤销指定 relay 的 reservation。
    ///
    /// 这会移除 circuit listener 与自动重建意图；若 helper 的拨号仍在进行且
    /// 没有其他调用者，会一并中止该拨号。已建立的其他业务连接不会被影响。
    pub async fn cancel_relay_reservation(&self, relay: NodeId) -> Result<(), Error> {
        self.request(|reply| ActorMessage::CancelRelayReservation { relay, reply })
            .await
    }

    /// 注册基础设施节点（bootstrap / LanHelper / 自建 relay）：进地址簿与
    /// kad 路由表、拨号，并按角色请求 reservation（断线后幂等重建）。
    pub async fn add_infrastructure_peer(
        &self,
        peer: impl Into<NodeAddr>,
        roles: InfraRoles,
    ) -> Result<(), Error> {
        let peer = peer.into();
        self.request(|reply| ActorMessage::AddInfraPeer { peer, roles, reply })
            .await
    }

    /// 注销基础设施节点——[`add_infrastructure_peer`](Self::add_infrastructure_peer)
    /// 的对称面：撤销 relay 常驻意图、清地址簿与 kad 路由表、关闭对应
    /// circuit listener，并**立刻断开**与该节点的全部连接（含中止在途拨号，
    /// pin 93c5059 的 `Pool::disconnect` 对 pending 连接调用 abort）。
    ///
    /// 注销后内核不再存在任何针对该节点的自动重连或 reservation 重建路径。
    pub async fn remove_infrastructure_peer(&self, node: NodeId) -> Result<(), Error> {
        self.request(|reply| ActorMessage::RemoveInfraPeer { node, reply })
            .await
    }

    /// DHT 操作句柄（Builder 未启用 DHT 时为 `None`）。
    pub fn dht(&self) -> Option<&crate::dht::Dht> {
        self.inner.dht.as_ref()
    }

    // ── 流 ──

    /// 向对端打开一条协议流（数据面入口）。
    ///
    /// 未连接时先 `connect`（复刻旧栈 request-response 的按需拨号语义）。
    pub async fn open(&self, node: NodeId, protocol: ProtocolId) -> Result<P2pStream, OpenError> {
        if self.inner.closed.is_cancelled() {
            return Err(OpenError::Closed);
        }
        let peer = *node.as_peer_id();
        let guard = self
            .inner
            .registry
            .try_acquire(peer, protocol.clone(), Direction::Outbound)
            .ok_or_else(|| OpenError::LimitExceeded(protocol.clone()))?;

        if !self.is_connected(node) {
            self.connect(node).await?;
        }

        // 热路径零分配：'static 协议（本项目全部如此）直接 `new(&'static str)`，
        // 免每流一次 `String` 堆分配；动态协议才回退 owned 转换。
        let stream_protocol = match protocol.as_static() {
            Some(s) => StreamProtocol::new(s),
            None => StreamProtocol::try_from_owned(protocol.as_str().to_owned())
                .expect("ProtocolId guarantees '/' prefix"),
        };
        let mut control = self.inner.control.clone();
        match control.open_stream(peer, stream_protocol).await {
            Ok(stream) => Ok(P2pStream::new(
                node,
                protocol,
                Direction::Outbound,
                stream,
                Some(guard),
            )),
            Err(libp2p_stream::OpenStreamError::UnsupportedProtocol(_)) => {
                Err(OpenError::UnsupportedProtocol(protocol))
            }
            Err(libp2p_stream::OpenStreamError::Io(e)) => Err(OpenError::Io(e)),
            Err(e) => Err(OpenError::Io(std::io::Error::other(e.to_string()))),
        }
    }

    // ── 状态观察 ──

    /// 本机监听/外部地址。
    pub fn watch_addrs(&self) -> Watcher<AddrsInfo> {
        Watcher::new(self.inner.watch_addrs.clone())
    }

    /// NAT 状态（AutoNAT 探测，M2 接入写端）。
    pub fn watch_nat(&self) -> Watcher<NatStatus> {
        Watcher::new(self.inner.watch_nat.clone())
    }

    /// 各对端连接状态表（最优路径 + RTT 采样）。
    pub fn watch_conns(&self) -> Watcher<BTreeMap<NodeId, ConnInfo>> {
        Watcher::new(self.inner.watch_conns.clone())
    }

    /// relay reservation 状态表。
    pub fn watch_relays(&self) -> Watcher<BTreeMap<NodeId, RelayState>> {
        Watcher::new(self.inner.watch_relays.clone())
    }

    /// 订阅必达边沿事件。
    pub async fn subscribe(&self) -> Result<Events, Error> {
        let (tx, rx) = subscriber_channel();
        self.inner
            .actor_tx
            .send(ActorMessage::Subscribe { tx })
            .await
            .map_err(|_| Error::Closed)?;
        Ok(Events::new(rx))
    }

    // ── 关停 ──

    /// 优雅关停：停 actor（drop Swarm 即断开全部连接）、唤醒 `closed()`。
    ///
    /// 幂等；Router 的关停由 `Router::shutdown` 单独编排（先于此调用）。
    pub async fn close(&self) {
        let _ = self.inner.actor_tx.send(ActorMessage::Shutdown).await;
        let handle = self.inner.actor_handle.lock().expect("lock").take();
        if let Some(handle) = handle {
            // actor 收到 Shutdown 必然退出；超时只可能是 swarm drop 阻塞，任其 detach
            let _ = n0_future::time::timeout(SHUTDOWN_GRACE, handle).await;
        }
        self.inner.closed.cancel();
    }

    /// 关停完成信号（`close()` 结束后 resolve）。
    pub fn closed(&self) -> tokio_util::sync::WaitForCancellationFuture<'_> {
        self.inner.closed.cancelled()
    }

    // ── 内部 ──

    async fn request<T>(
        &self,
        make: impl FnOnce(oneshot::Sender<Result<T, Error>>) -> ActorMessage,
    ) -> Result<T, Error> {
        let (tx, rx) = oneshot::channel();
        self.inner
            .actor_tx
            .send(make(tx))
            .await
            .map_err(|_| Error::Closed)?;
        rx.await.map_err(|_| Error::Closed)?
    }

    pub(crate) fn from_inner(inner: Inner) -> Self {
        Self {
            inner: Arc::new(inner),
        }
    }

    /// Router 注册入站协议用。
    pub(crate) fn stream_control(&self) -> libp2p_stream::Control {
        self.inner.control.clone()
    }

    pub(crate) fn registry(&self) -> StreamRegistry {
        self.inner.registry.clone()
    }
}
