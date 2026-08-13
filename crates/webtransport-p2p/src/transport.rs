//! `libp2p_core::Transport` 实现 —— 三层的组装点。
//!
//! # 证书轮换如何对外表达
//!
//! **不发明任何新机制**：`TransportEvent` 本来就有 `AddressExpired` / `NewAddress`，
//! 轮换要说的正是这件事。
//!
//! ```text
//! Rotation::advance() → Rotated
//!     ↓
//! AddressExpired(…/certhash/A/certhash/B)
//! NewAddress   (…/certhash/B/certhash/C)
//! ```
//!
//! 上层（identify、地址收集、bootstrap 通告）走的是与「网卡插拔」完全相同的路径，
//! 无需为证书这件事写任何特殊分支。
//!
//! # 时钟由 poll 驱动，但不是每次 poll 都读时钟
//!
//! 决策是「不起后台定时任务」——多一个 task 就多一处生命周期与泄漏风险，而
//! `webrtc-p2p` 那轮的教训正是「别把 transport 驱动和别的东西绑在额外的 task 上」。
//!
//! 实现上没有退化成「每次 poll 都 `SystemTime::now()`」：poll 在空闲连接上每秒可达上千次，
//! 而轮换周期是 14 天。中间隔了一个 [`ROTATION_CHECK_INTERVAL`] 的 `Delay` —— 它由
//! poll 自己驱动（**不是 task**），顺带解决了纯「顺带检查」方案的短板：`Delay` 会注册
//! waker，因此**完全空闲时也保证被唤醒**，不必指望恰好有别的事情触发一次 poll。

use std::collections::{HashMap, VecDeque};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};
use std::time::SystemTime;

use futures::FutureExt;
use futures::future::BoxFuture;
use futures_timer::Delay;
use libp2p_core::multiaddr::Multiaddr;
use libp2p_core::transport::{DialOpts, ListenerId, TransportError, TransportEvent};
use libp2p_identity::PeerId;
use wtransport::Connection;

use crate::addr::{self, Certhash};
use crate::certificate::{Advance, Rotation};
use crate::config::{Config, ROTATION_CHECK_INTERVAL};
use crate::dialer;
use crate::error::Error;
use crate::listener::Listener;
use crate::muxer::{Muxer, Substream};
use crate::noise;

/// 一条连接从「会话已建立」到「身份已认证」的过程。
pub type Upgrade = BoxFuture<'static, Result<(PeerId, Muxer), Error>>;

type Event = TransportEvent<Upgrade, Error>;

/// libp2p WebTransport 传输（native）。
pub struct Transport {
    /// `Arc` 而非值：`upgrade_inbound` 每条入站连接都要把它捕获进 upgrade future，
    /// 而 `Config` 里装着 `Keypair`（含 Ed25519 私钥）。按值 clone 等于**每条未完成的
    /// 入站连接各持一份私钥副本**，活到握手完成或 20s 超时。
    config: Arc<Config>,
    rotation: Rotation,
    listeners: HashMap<ListenerId, Listener>,
    /// 待交付的事件。
    ///
    /// `listen_on` / `remove_listener` 是**同步**调用，产出的事件只能先攒着 —— 这也是
    /// 为什么必须存 waker 并在入队时唤醒（见 [`Self::queue`]）。
    pending: VecDeque<Event>,
    waker: Option<Waker>,
    rotation_check: Delay,
}

impl Transport {
    /// 装配一个 transport。
    ///
    /// 证书在这里就绪：能从持久化端口读回就复用（重启后 certhash 不变），读不回或读到的
    /// 已不可用就重新生成。**构造期失败远好过第一次 `listen_on` 时才炸** —— 那时调用方
    /// 通常已经把它塞进 Swarm 了。
    pub fn new(config: Config) -> Result<Self, Error> {
        let now = SystemTime::now();
        let Loaded {
            mut rotation,
            needs_persist,
        } = load_or_bootstrap(&config, now)?;

        // 读回来的证书可能已经过期（停机期间跨过了轮换点）。修复只有 advance 一条路径，
        // load 本身刻意不判过期，两者职责因此不重叠。
        let advanced = rotation.advance(now)?;

        let transport = Self {
            config: Arc::new(config),
            rotation,
            listeners: HashMap::new(),
            pending: VecDeque::new(),
            waker: None,
            rotation_check: Delay::new(ROTATION_CHECK_INTERVAL),
        };

        // 此时还没有 listener，无需发地址事件。
        //
        // ⚠️ `needs_persist` 必须由 `load_or_bootstrap` **回报**，不能事后再 `load()` 一次
        // 去猜。「磁盘上有东西」不等于「用上了它」：PEM 损坏那条路径上 `load()` 返回
        // `Ok(Some(垃圾))` 而实际走的是重新生成 —— 事后猜会判成「不是新的」→ 不落盘 →
        // 下次启动再坏一次、再生成一次，certhash **每次重启都变**。
        if needs_persist || matches!(advanced, Advance::Rotated { .. }) {
            transport.persist();
        }
        Ok(transport)
    }

    /// 本机当前通告的证书哈希（`current` 在前）。诊断与测试用。
    pub fn certhashes(&self) -> Vec<Certhash> {
        self.rotation.advertised()
    }

    /// `current` 证书的过期时刻。诊断用（「还有几天轮换」）。
    pub fn certificate_expires_at(&self) -> SystemTime {
        self.rotation.current_expires_at()
    }

    /// 推进轮换状态机，并把结果翻译成地址事件。
    ///
    /// 三件事都可能失败，但**没有一件应该让节点停摆**：
    ///
    /// | 失败 | 处理 | 理由 |
    /// |---|---|---|
    /// | 生成新证书 | `error!` 并保持原状 | 原证书还没过期就崩掉更糟；下个周期重试 |
    /// | `reload_config` | `error!`，该 listener 继续用旧证书 | 旧证书刚过期，但连接还能建（对端会拒），比端口直接死好 |
    /// | 持久化写入 | `warn!` | 内存里的证书是好的，后果只是下次启动 certhash 变 |
    fn advance_rotation(&mut self, now: SystemTime) {
        let advanced = match self.rotation.advance(now) {
            Ok(advanced) => advanced,
            Err(e) => {
                tracing::error!(error = %e, "webtransport 证书轮换失败，本轮维持原证书");
                return;
            }
        };
        let Advance::Rotated { retired } = advanced else {
            return;
        };

        // 字段级借用：`rotation` 与 `listeners`、`pending` 各借各的。
        let Self {
            rotation,
            listeners,
            pending,
            ..
        } = self;
        let certhashes = rotation.advertised();

        for (id, listener) in listeners.iter_mut() {
            match listener.reload(rotation.server_identity(), &certhashes) {
                Ok(expired) => {
                    // 先过期后新增：反过来的话，上层在两条事件之间会短暂地同时持有新旧
                    // 两组地址，而其中一组已经不可用。
                    for listen_addr in expired {
                        pending.push_back(TransportEvent::AddressExpired {
                            listener_id: *id,
                            listen_addr,
                        });
                    }
                    for listen_addr in listener.announced() {
                        pending.push_back(TransportEvent::NewAddress {
                            listener_id: *id,
                            listen_addr: listen_addr.clone(),
                        });
                    }
                }
                Err(e) => tracing::error!(
                    listener = ?id,
                    error = %e,
                    "webtransport 换证书失败，该监听端口仍在用已过期的证书"
                ),
            }
        }

        tracing::info!(retired = ?retired, "webtransport 证书已轮换，通告地址已更新");
        self.persist();
    }

    /// 回写持久化。**失败不中断服务** —— 见 [`Self::advance_rotation`] 的表格。
    fn persist(&self) {
        let Some(store) = self.config.store() else {
            return;
        };
        if let Err(e) = store.store(&self.rotation.to_pem()) {
            tracing::warn!(
                error = %e,
                "webtransport 证书持久化失败：本次运行不受影响，但下次启动 certhash 会改变"
            );
        }
    }

    /// 入队一个事件并唤醒 poll。
    ///
    /// **唤醒不能省。** `listen_on` / `remove_listener` 是外部同步调用，此刻没有任何东西
    /// 会去 poll 我们；少了这一下，新监听地址要等到下次因别的原因被 poll 才通告得出去
    /// （`webrtc-p2p` 踩过同一个坑）。
    fn queue(&mut self, event: Event) {
        self.pending.push_back(event);
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }

    /// 把一条已完成 CONNECT 的入站会话，变成一条已认证的 libp2p 连接。
    fn upgrade_inbound(&self, session: Connection) -> Upgrade {
        let config = Arc::clone(&self.config);
        // `libp2p-noise` 的 `with_webtransport_certhashes` 收 by-value，这里只能 clone
        // 一次集合本身（4 个 Multihash）—— 但不必再重算一遍 SHA-256 与 wrap。
        let certhashes = self.rotation.noise_certhashes().clone();
        let timeout = config.handshake_timeout();

        async move {
            let handshake = async {
                // spec：客户端开的**第一条**流用于 Noise 握手。服务端在这里等它。
                let stream = session
                    .accept_bi()
                    .await
                    .map_err(|e| Error::Connect(format!("等待握手流失败：{e}")))?;

                let peer = noise::authenticate_inbound(
                    config.id_keys(),
                    Substream::new(stream),
                    certhashes,
                )
                .await?;

                Ok((peer, Muxer::new(session, None)))
            };

            crate::error::with_timeout(handshake, timeout).await
        }
        .boxed()
    }
}

/// [`load_or_bootstrap`] 的结果。
struct Loaded {
    rotation: Rotation,
    /// 内存里的这份**是否与持久化内容不一致**，即要不要立刻回写。
    ///
    /// 三条容易写错的边界，各自的答案都不一样：
    ///
    /// | 情形 | 回写？ | 理由 |
    /// |---|---|---|
    /// | 没存过 / 存量解析失败 | **是** | 磁盘上没有可用数据，不写就每次重启都重新生成 |
    /// | 存量只有一张证书（`next` 是现生成的） | **是** | 不写就自我延续：每次启动补一张不同的 next |
    /// | 存量完整 | 否 | 一致，写了也是同样内容 |
    /// | **`load()` 报 IO 错** | **否** | 那份数据可能完好无损，只是这次没读到 —— 覆盖它等于把一次瞬时故障变成永久的身份丢失 |
    needs_persist: bool,
}

/// 读回持久化的证书对；读不回或不可用就重新生成。
///
/// 三种"读不回"（没存过 / 存的东西坏了 / 存的证书不合规）走同一条兜底路径，因为对调用方
/// 而言后果完全相同：certhash 会变，且**必须立刻回写**。区别只体现在日志级别上。
fn load_or_bootstrap(config: &Config, now: SystemTime) -> Result<Loaded, Error> {
    let fresh = |rotation| {
        Ok(Loaded {
            rotation,
            needs_persist: true,
        })
    };

    let Some(store) = config.store() else {
        // 这里**不 warn**：拨号方本来就不需要持久化服务端证书，构造期还不知道它会不会
        // 监听。判据挪到 `listen_on` —— 只有真的要监听时，「没持久化」才是个问题。
        tracing::debug!("未接证书持久化端口，本次运行现生成证书");
        return fresh(Rotation::bootstrap(now)?);
    };

    match store.load() {
        Ok(Some(pem)) => match Rotation::from_pem(&pem) {
            Ok((rotation, complete)) => {
                if !complete {
                    tracing::warn!(
                        "持久化的 webtransport 证书不完整，已补齐并回写；\
                         上一轮通告出去的地址可能失效"
                    );
                }
                Ok(Loaded {
                    rotation,
                    needs_persist: !complete,
                })
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "持久化的 webtransport 证书不可用，重新生成；对端记下的旧地址将失效"
                );
                fresh(Rotation::bootstrap(now)?)
            }
        },
        Ok(None) => fresh(Rotation::bootstrap(now)?),
        Err(e) => {
            // ⚠️ **不回写。** 读失败不等于数据坏了 —— 文件可能被杀软临时锁住、权限被改、
            // 一次 EIO。覆盖它等于把一次瞬时故障变成**永久**的身份丢失，对端记下的地址
            // 全部作废。CLAUDE.md 为设备身份文件写死过同一条判据（「读取失败不降级」），
            // 这里是同一个坑。
            tracing::warn!(
                error = %e,
                "读取 webtransport 证书失败，本次运行用新证书，**不覆盖磁盘上的那份**"
            );
            Ok(Loaded {
                rotation: Rotation::bootstrap(now)?,
                needs_persist: false,
            })
        }
    }
}

impl libp2p_core::Transport for Transport {
    type Output = (PeerId, Muxer);
    type Error = Error;
    type ListenerUpgrade = Upgrade;
    type Dial = Upgrade;

    fn listen_on(
        &mut self,
        id: ListenerId,
        addr: Multiaddr,
    ) -> Result<(), TransportError<Self::Error>> {
        let socket =
            addr::parse_listen(&addr).ok_or(TransportError::MultiaddrNotSupported(addr))?;

        let listener = Listener::bind(
            socket,
            self.rotation.server_identity(),
            &self.rotation.advertised(),
        )
        .map_err(TransportError::Other)?;

        if self.config.store().is_none() {
            // 只有监听方才有「服务端证书」这回事，所以这条判据必须在这里而不是构造期
            // —— 放构造期的话，纯拨号方也会看到它，久而久之就没人当真了。
            tracing::warn!(
                bound = %listener.bound(),
                "WebTransport 正在监听但未接证书持久化端口：重启后 certhash 会变，\
                 对端已记下的地址将全部失效。生产环境请用 `Config::with_certificate_store`"
            );
        }

        tracing::debug!(
            bound = %listener.bound(),
            addrs = listener.announced().len(),
            "webtransport 开始监听"
        );

        for listen_addr in listener.announced().to_vec() {
            self.queue(TransportEvent::NewAddress {
                listener_id: id,
                listen_addr,
            });
        }
        self.listeners.insert(id, listener);
        Ok(())
    }

    fn remove_listener(&mut self, id: ListenerId) -> bool {
        let Some(listener) = self.listeners.remove(&id) else {
            return false;
        };

        // 通告过的地址要逐条撤回：Swarm 不会因为 listener 没了就自动认为它们失效。
        for listen_addr in listener.announced().to_vec() {
            self.queue(TransportEvent::AddressExpired {
                listener_id: id,
                listen_addr,
            });
        }
        self.queue(TransportEvent::ListenerClosed {
            listener_id: id,
            reason: Ok(()),
        });
        true
    }

    fn dial(
        &mut self,
        addr: Multiaddr,
        _opts: DialOpts,
    ) -> Result<Self::Dial, TransportError<Self::Error>> {
        // `DialOpts::port_use` 被忽略：`PortUse::Reuse` 要求从监听端口发起拨号，而
        // wtransport 不允许复用已有的 UDP socket（正是 rust-libp2p PR #4348 卡住的那点）。
        // 忽略而不是报错 —— 它只是个优化提示，做不到就走独立端口。
        let target = addr::parse_dial(&addr).ok_or(TransportError::MultiaddrNotSupported(addr))?;

        Ok(dialer::dial(target, Arc::clone(&self.config)).boxed())
    }

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Event> {
        let this = self.get_mut();

        loop {
            if let Some(event) = this.pending.pop_front() {
                return Poll::Ready(event);
            }

            // 轮换检查。Delay 由这里驱动（不是 task），且它注册了 waker，
            // 因此完全空闲时也保证被唤醒。
            if this.rotation_check.poll_unpin(cx).is_ready() {
                this.rotation_check.reset(ROTATION_CHECK_INTERVAL);
                this.advance_rotation(SystemTime::now());
                continue;
            }

            let mut closed = None;
            let mut incoming = None;

            for (id, listener) in this.listeners.iter_mut() {
                match listener.poll_incoming(cx) {
                    Poll::Ready(Some(session)) => {
                        incoming = Some((*id, session, listener.announced()[0].clone()));
                        break;
                    }
                    // accept 循环没了（只可能是它 panic 了）。端口已死，据实上报。
                    Poll::Ready(None) => {
                        closed = Some(*id);
                        break;
                    }
                    Poll::Pending => {}
                }
            }

            if let Some(id) = closed {
                this.listeners.remove(&id);
                this.pending.push_back(TransportEvent::ListenerClosed {
                    listener_id: id,
                    reason: Err(Error::Connect("accept 循环意外结束".into())),
                });
                continue;
            }

            if let Some((listener_id, session, local_addr)) = incoming {
                let remote = session.remote_address();
                return Poll::Ready(TransportEvent::Incoming {
                    listener_id,
                    upgrade: this.upgrade_inbound(session),
                    local_addr,
                    // 对端的 certhash 我们无从得知（它没写在任何地方），留空。
                    // 官方 webrtc-direct 的 `send_back_addr` 同样如此。
                    send_back_addr: addr::advertise_addr(remote, &[]),
                });
            }

            // 先比 `will_wake`：Swarm 每轮都会 poll 到这里，而 waker 通常是同一个。
            // 无条件 clone 等于每轮 2 次原子 RMW（新的 clone + 旧的 drop）。
            // 同一个函数里 poll 的 `futures_timer::Delay` 内部就是这么做的。
            if !this.waker.as_ref().is_some_and(|w| w.will_wake(cx.waker())) {
                this.waker = Some(cx.waker().clone());
            }
            return Poll::Pending;
        }
    }
}

impl std::fmt::Debug for Transport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Transport")
            .field("listeners", &self.listeners.len())
            .field("certhashes", &self.rotation.advertised())
            .finish()
    }
}

/// 轮换的行为测试。
///
/// **必须住在 crate 内**：验证「跨过期切换」要把时钟推到 14 天后，而对外只有 poll 驱动的
/// 入口（那个入口读真实时钟）。`advance_rotation(now)` 是私有的，集成测试够不着它 ——
/// 这正是「时钟从参数进来」这条设计约束换来的东西：否则这一整节测试根本写不出来。
#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use futures::AsyncReadExt;
    use futures::AsyncWriteExt;
    use futures::future::poll_fn;
    use libp2p_core::Transport as _;
    use libp2p_core::muxing::StreamMuxer;
    use libp2p_core::transport::{DialOpts, PortUse};
    use libp2p_identity::Keypair;

    use super::*;
    use crate::config::{CertificateStore, MemoryCertificateStore, StoreError};

    /// 等一个事件的上限。取大是为了容忍 CI 上的慢机器，反正正常路径瞬间返回。
    const EVENT_TIMEOUT: Duration = Duration::from_secs(10);

    /// 写入必失败的 store，用来验证「持久化失败不中断服务」。
    struct FailingStore;

    impl CertificateStore for FailingStore {
        fn load(&self) -> Result<Option<String>, StoreError> {
            Ok(None)
        }
        fn store(&self, _pem: &str) -> Result<(), StoreError> {
            Err(StoreError::new(std::io::Error::other("磁盘满了")))
        }
    }

    fn transport() -> Transport {
        Transport::new(Config::new(Keypair::generate_ed25519())).unwrap()
    }

    /// 等一个 transport 事件。
    ///
    /// ⚠️ **超时不能省。** `poll` 在没有事件时返回 `Pending`，于是「本该发事件却没发」
    /// 这类 bug 的表现是**永久挂起**而不是断言失败 —— 在 CI 里就是一句「job 超时」，
    /// 看不出是谁挂的。实测：把 `reload_config` 的 rebind 改成 true，reload 会失败，
    /// 轮换于是一条事件都不发，本 helper 是唯一能把它变成清晰失败的地方。
    async fn next_event(transport: &mut Transport) -> Event {
        tokio::time::timeout(
            EVENT_TIMEOUT,
            poll_fn(|cx| Pin::new(&mut *transport).poll(cx)),
        )
        .await
        .expect("等待 transport 事件超时：预期的事件根本没有被发出")
    }

    async fn listen(transport: &mut Transport) -> (ListenerId, Multiaddr) {
        let id = ListenerId::next();
        transport
            .listen_on(
                id,
                "/ip4/127.0.0.1/udp/0/quic-v1/webtransport".parse().unwrap(),
            )
            .unwrap();
        match next_event(transport).await {
            TransportEvent::NewAddress { listen_addr, .. } => (id, listen_addr),
            other => panic!("应先通告地址，实得 {other:?}"),
        }
    }

    fn dial_opts() -> DialOpts {
        DialOpts {
            role: libp2p_core::Endpoint::Dialer,
            port_use: PortUse::New,
        }
    }

    /// 跨过期后：先撤回旧地址，再通告新地址，且新地址的 current 就是原来的 next。
    ///
    /// 顺序不能反 —— 反了的话上层在两条事件之间会同时持有新旧两组地址，而旧的那组
    /// 已经不可用。
    #[tokio::test]
    async fn rotation_expires_old_address_then_announces_new() {
        let mut transport = transport();
        let (listener, old_addr) = listen(&mut transport).await;
        let old_hashes = transport.certhashes();

        transport.advance_rotation(transport.certificate_expires_at() + Duration::from_secs(1));

        match next_event(&mut transport).await {
            TransportEvent::AddressExpired {
                listener_id,
                listen_addr,
            } => {
                assert_eq!(listener_id, listener);
                assert_eq!(listen_addr, old_addr, "撤回的必须是此前通告出去的那一条");
            }
            other => panic!("应先撤回旧地址，实得 {other:?}"),
        }

        let new_addr = match next_event(&mut transport).await {
            TransportEvent::NewAddress { listen_addr, .. } => listen_addr,
            other => panic!("应随即通告新地址，实得 {other:?}"),
        };
        assert_ne!(new_addr, old_addr);

        let fresh = transport.certhashes();
        assert_eq!(
            fresh[0], old_hashes[1],
            "原来的 next 必须原样提升为 current"
        );
        assert!(!old_hashes.contains(&fresh[1]), "新的 next 必须是新生成的");
    }

    /// 有效期内推进不得产生任何事件 —— 否则周期性的检查会刷屏地址变更。
    #[tokio::test]
    async fn rotation_within_validity_emits_nothing() {
        let mut transport = transport();
        let (_, addr) = listen(&mut transport).await;

        transport.advance_rotation(SystemTime::now() + Duration::from_secs(60));

        assert!(transport.pending.is_empty(), "不该有事件");
        assert_eq!(
            transport.listeners.values().next().unwrap().announced(),
            [addr],
            "通告地址不该变"
        );
    }

    /// ★ 轮换时**既有连接不断**。这是选 `wtransport` 的决定性理由
    /// （`reload_config(_, rebind = false)`），也是验收标准里的一条。
    #[tokio::test]
    async fn rotation_keeps_existing_connections_alive() {
        let mut server = transport();
        let (_, addr) = listen(&mut server).await;
        let mut client = Transport::new(Config::new(Keypair::generate_ed25519())).unwrap();

        // 建连要同时驱动两侧：server 的 poll 产出 Incoming，client 的 dial 是个 future。
        let (server_side, client_side) = futures::future::join(
            async {
                loop {
                    if let TransportEvent::Incoming { upgrade, .. } = next_event(&mut server).await
                    {
                        return upgrade.await.expect("入站升级应成功");
                    }
                }
            },
            async {
                client
                    .dial(addr, dial_opts())
                    .expect("地址应被认领")
                    .await
                    .expect("拨号应成功")
            },
        )
        .await;

        // 连接已建立。此刻轮换。
        transport_advance(&mut server);

        // 老连接照常收发。
        let (_, server_muxer) = server_side;
        let (_, client_muxer) = client_side;
        futures::pin_mut!(server_muxer);
        futures::pin_mut!(client_muxer);

        let echo = async {
            let mut stream = poll_fn(|cx| server_muxer.as_mut().poll_inbound(cx))
                .await
                .expect("轮换后仍应能收到子流");
            let mut buf = [0u8; 4];
            stream.read_exact(&mut buf).await.unwrap();
            stream.write_all(&buf).await.unwrap();
            stream.flush().await.unwrap();
            std::future::pending::<()>().await;
        };

        let roundtrip = async {
            let mut stream = poll_fn(|cx| client_muxer.as_mut().poll_outbound(cx))
                .await
                .expect("轮换后仍应能开子流");
            stream.write_all(b"ping").await.unwrap();
            stream.flush().await.unwrap();
            let mut buf = [0u8; 4];
            stream.read_exact(&mut buf).await.unwrap();
            buf
        };

        futures::pin_mut!(echo);
        let exchange = async {
            match futures::future::select(roundtrip.boxed(), echo).await {
                futures::future::Either::Left((got, _)) => got,
                futures::future::Either::Right(_) => unreachable!("echo 永不返回"),
            }
        };

        // ⚠️ 超时不能省。连接被轮换搞断时，`read_exact` 是**永久挂起**而不是报错 ——
        // 少了这层，本条测试在 CI 里的失败形态是「整个 job 超时」，看不出是谁挂的。
        // （实测：把 `reload_config` 的 rebind 改成 true 就会走到这条路径。）
        let got = tokio::time::timeout(Duration::from_secs(10), exchange)
            .await
            .expect("轮换后既有连接必须仍可收发，超时说明它被搞断了");

        assert_eq!(&got, b"ping", "轮换不得影响既有连接上的数据面");
    }

    fn transport_advance(transport: &mut Transport) {
        transport.advance_rotation(transport.certificate_expires_at() + Duration::from_secs(1));
    }

    /// 轮换后必须回写持久化，否则重启会退回到轮换前的证书对。
    #[tokio::test]
    async fn rotation_persists_the_new_pair() {
        let store = MemoryCertificateStore::default();
        let mut transport = Transport::new(
            Config::new(Keypair::generate_ed25519())
                .with_certificate_store(Arc::new(store.clone())),
        )
        .unwrap();

        let before = store.load().unwrap().expect("首次装配就该落盘");
        transport_advance(&mut transport);
        let after = store.load().unwrap().unwrap();

        assert_ne!(after, before, "轮换后必须回写");
        let (restored, complete) = Rotation::from_pem(&after).unwrap();
        assert!(complete, "回写的内容必须是完整的");
        assert_eq!(
            restored.advertised(),
            transport.certhashes(),
            "落盘的必须是轮换后的那一对"
        );
        // ★ 退役证书也要在里面 —— 少了它，一次重启就把「旧地址能撑过一整轮」打掉：
        // 对端拿旧地址来拨，TLS 过得去（服务端出示的 current 在它的接受集合内），
        // Noise 却会失败（判据是「期望集合 ⊆ 收到集合」）。
        assert_eq!(
            restored.noise_certhashes(),
            transport.rotation.noise_certhashes(),
            "退役 certhash 必须跟着一起持久化"
        );
    }

    /// 持久化失败只是记 warn —— 内存里的证书是好的，节点必须照常服务。
    /// 把它当致命错误的话，一次磁盘满就会让节点起不来。
    #[tokio::test]
    async fn persistence_failure_does_not_break_the_transport() {
        let mut transport = Transport::new(
            Config::new(Keypair::generate_ed25519()).with_certificate_store(Arc::new(FailingStore)),
        )
        .expect("写入失败不该让装配失败");

        let (_, addr) = listen(&mut transport).await;
        transport_advance(&mut transport);

        // 轮换照常发生并照常通告。
        assert!(matches!(
            next_event(&mut transport).await,
            TransportEvent::AddressExpired { .. }
        ));
        match next_event(&mut transport).await {
            TransportEvent::NewAddress { listen_addr, .. } => assert_ne!(listen_addr, addr),
            other => panic!("实得 {other:?}"),
        }
    }

    /// ★ 持久化数据损坏时，重新生成的证书**必须落盘**。
    ///
    /// 不落盘的后果不是「这次没存上」，而是**每次重启 certhash 都变**：下次启动读到同一份
    /// 垃圾、再生成一份、再不落盘…… 而「重启后 certhash 不变」正是这个持久化端口存在的
    /// 唯一理由。第二段断言（重启后一致）才是真正在验这件事。
    #[tokio::test]
    async fn corrupt_persisted_data_is_replaced_and_written_back() {
        let store = MemoryCertificateStore::default();
        store.store("这不是一份 PEM").unwrap();

        let first = Transport::new(
            Config::new(Keypair::generate_ed25519())
                .with_certificate_store(Arc::new(store.clone())),
        )
        .unwrap();
        let hashes = first.certhashes();

        let persisted = store.load().unwrap().expect("必须回写");
        assert!(
            persisted.contains("BEGIN CERTIFICATE"),
            "损坏的数据必须被换成真证书，实得 {persisted:?}"
        );

        let second = Transport::new(
            Config::new(Keypair::generate_ed25519())
                .with_certificate_store(Arc::new(store.clone())),
        )
        .unwrap();
        assert_eq!(
            second.certhashes(),
            hashes,
            "回写之后，重启就该复用同一对证书"
        );
    }

    /// 装配时读到的证书若已过期，必须当场修好 —— 停机跨过轮换点是最常见的情形。
    #[tokio::test]
    async fn startup_repairs_expired_certificates() {
        let store = MemoryCertificateStore::default();
        // 造一份"很久以前"的证书对。
        let stale =
            Rotation::bootstrap(SystemTime::now() - Duration::from_secs(60 * 24 * 3600)).unwrap();
        let stale_hashes = stale.advertised();
        store.store(&stale.to_pem()).unwrap();

        let transport = Transport::new(
            Config::new(Keypair::generate_ed25519())
                .with_certificate_store(Arc::new(store.clone())),
        )
        .unwrap();

        assert!(
            transport
                .certhashes()
                .iter()
                .all(|h| !stale_hashes.contains(h)),
            "过期证书必须被换掉"
        );
        assert!(
            transport.certificate_expires_at() > SystemTime::now(),
            "修好之后 current 必须是有效的"
        );
        // ⚠️ 这里**不能**只断言「store 非空」—— 它在测试开头就被填过了，那样的断言恒真：
        // 把 `Transport::new` 里的 `|| matches!(advanced, Rotated)` 整条删掉，测试照样绿。
        // 必须比对落盘内容与内存状态。
        let (persisted, _) = Rotation::from_pem(&store.load().unwrap().unwrap()).unwrap();
        assert_eq!(
            persisted.advertised(),
            transport.certhashes(),
            "修复后的证书对必须落盘，否则每次启动都重建一次、certhash 每次都变"
        );
    }
}
