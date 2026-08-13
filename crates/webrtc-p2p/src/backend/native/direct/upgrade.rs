//! direct 模式的建连流程。
//!
//! 两个方向的步骤是**镜像**的，差别集中在三处（见下表）。别为了消重把它们合成一个
//! 带 `role: bool` 的函数——那三处差异每一处都涉及安全语义，摊平后极易改错一边。
//!
//! | | dialer（客户端） | listener（服务端） |
//! |---|---|---|
//! | ufrag | 自己随机生成 | **从入站 STUN 包里学到** |
//! | SDP | 真 offer + 构造的 answer | 构造的 offer + 真 answer |
//! | ICE | 完整（但只有一个 host candidate 可用） | **lite**，只被动应答 |
//! | 对端指纹 | 来自 multiaddr 的 certhash（校验） | 握手后从 DTLS 取（不预先校验） |
//!
//! # 为什么最后一定要跑 Noise
//!
//! certhash 写在 multiaddr 里，而 multiaddr 可以经任何信道传播——贴在网页上、印在
//! 二维码里、被中间人替换掉。DTLS 只能证明「对面持有这张证书」，证明不了「这张证书
//! 属于那个 PeerId」。Noise 握手把 DTLS 指纹（双方的）压进 prologue，两端算出的
//! prologue 必须一致，于是身份与信道被绑在一起。
//!
//! 打洞模式没有这一步，因为那边的 SDP 走已认证的 relay 连接（spec FAQ 第一条）。

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use futures::channel::mpsc;
use futures::future::Either;
use futures_timer::Delay;
use libp2p_identity::{Keypair, PeerId};
use libp2p_webrtc_utils::{Fingerprint, StreamConfig, noise};
use webrtc::data_channel::DataChannel;
use webrtc::peer_connection::{
    MediaEngine, PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler,
    RTCConfigurationBuilder, RTCPeerConnectionState, RTCStatsReportEntry, Registry, SettingEngine,
    StatsSelector, register_default_interceptors,
};
use webrtc::runtime::Runtime;

// 这几个 `webrtc` 没有 re-export，只能从底层的 sans-io 核心直接取。
// **一律经 `rtc::` 转出**（它 re-export 了全套子 crate），不要直接依赖 `rtc-ice`：
// 那样一旦 rtc 被 `[patch]` 换成 git 源，两边会解析成两个不同的同名类型。
use rtc::data_channel::{RTCDataChannelId, RTCDataChannelInit};
use rtc::ice::mdns::MulticastDnsMode;
use rtc::ice::network_type::NetworkType;
use rtc::peer_connection::transport::RTCDtlsRole;

use super::certificate::Certificate;
use super::sdp;
use super::udp_mux::{MuxedRuntime, MuxedSocket};
use crate::backend::native::data_channel;
// 两个调优常量与打洞路径共用——它们来自同一次 spike 实测，分两份写迟早只改一处。
use crate::backend::native::managed::ManagedPeerConnection;
use crate::backend::native::muxer::Muxer;
use crate::backend::native::{SEND_BUFFER_LIMIT, sctp_receive_buffer};
use crate::error::Error;
use crate::swarm::Connection;
use crate::swarm::direct::{StateEvent, await_connected};

/// 从 DTLS 完成到 Noise 握手结束的整体超时。
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(20);

/// 一次建连所需的、与方向无关的材料。
#[derive(Clone)]
pub(crate) struct Context {
    /// 本机 DTLS 证书。**必须与通告地址里的 certhash 是同一张**。
    pub(crate) certificate: Certificate,
    /// 本机 libp2p 身份，用于 Noise 握手。
    pub(crate) id_keys: Keypair,
    pub(crate) stream_config: StreamConfig,
    /// 宿主的 async 运行时（`webrtc` 的抽象，非 tokio 直用）。
    pub(crate) runtime: Arc<dyn Runtime>,
}

/// 作为服务端接一条入站连接。
///
/// `ufrag` 来自 [`super::udp_mux::UdpMuxEvent::NewAddr`]——客户端在它本地构造的 SDP
/// 里选定，经首个 STUN binding request 的 `USERNAME` 属性带过来。
pub(crate) async fn inbound(
    remote: SocketAddr,
    ufrag: String,
    socket: Arc<MuxedSocket>,
    ctx: Context,
) -> Result<(PeerId, Connection), Error> {
    tracing::debug!(%remote, %ufrag, "webrtc-direct: 接入站连接");

    let mut se = base_setting_engine(&ufrag, remote);
    // ICE-lite：只被动应答连通性检查，不主动收集也不发起。服务端本来就有可达地址。
    se.set_lite(true);
    // 客户端的指纹此刻还不知道（构造的 offer 里填的是占位值），只能关掉校验，
    // 改由随后的 Noise 握手认证身份。
    se.disable_certificate_fingerprint_verification(true);
    // 明确当 DTLS server（等对方先打招呼）。
    //
    // ⚠️ 官方注释写着：去掉这行 DTLS 会建不起来——两端都发 `ClientHello`，谁也不应答。
    se.set_answering_dtls_role(RTCDtlsRole::Server)
        .map_err(|e| Error::Connection(format!("设置 DTLS 角色失败：{e}")))?;

    let (pc, mut events, inbound_dc) = build(se, Some(socket), remote, &ctx).await?;
    // 守卫必须紧跟 `build`：到 `finish` 之间的每个 `?` 都会 drop 掉 pc，而不关闭它
    // 就等于泄漏一个空转的 driver 任务（见 `managed` 模块文档）。
    let pc = ManagedPeerConnection::new(pc, ctx.runtime.clone());
    let noise_channel = create_noise_channel(pc.as_ref()).await?;

    // 顺序与 dialer 相反：先收下（构造的）offer，再生成真 answer。
    pc.set_remote_description(
        sdp::offer(remote, &ufrag, ctx.stream_config)
            .map_err(|e| Error::Connection(e.to_string()))?,
    )
    .await
    .map_err(|e| Error::Connection(format!("设置构造的 offer 失败：{e}")))?;
    let answer = pc
        .create_answer(None)
        .await
        .map_err(|e| Error::Connection(format!("创建 answer 失败：{e}")))?;
    pc.set_local_description(answer)
        .await
        .map_err(|e| Error::Connection(format!("设置本地 answer 失败：{e}")))?;

    with_timeout(await_connected(&mut events)).await?;

    with_timeout(await_channel_open(&noise_channel)).await?;

    // 指纹要在 DTLS 完成**之后**取——此前远端证书还不存在。
    let client_fingerprint = remote_fingerprint(pc.as_ref()).await?;
    let server_fingerprint = ctx.certificate.fingerprint();

    let noise_channel_id = noise_channel.id();
    let stream = wrap_noise_channel(noise_channel, &ctx);
    let (peer, stream_config) = with_timeout(noise::inbound_with_message_size(
        ctx.id_keys.clone(),
        stream,
        client_fingerprint,
        server_fingerprint,
        ctx.stream_config,
    ))
    .await?;

    tracing::debug!(%remote, %peer, "webrtc-direct: 入站连接就绪");
    Ok((
        peer,
        finish(pc, inbound_dc, stream_config, peer, noise_channel_id),
    ))
}

/// 作为客户端拨一条出站连接。
///
/// `server_fingerprint` 来自地址里的 certhash。它会经 DTLS 校验，所以拨到冒名顶替的
/// 服务端会在握手阶段失败——但**这只保证了「证书没被换」，不保证「对面是那个 PeerId」**，
/// 后者由 Noise 完成。
pub(crate) async fn outbound(
    remote: SocketAddr,
    server_fingerprint: Fingerprint,
    ctx: Context,
) -> Result<(PeerId, Connection), Error> {
    let ufrag = libp2p_webrtc_utils::sdp::random_ufrag();
    tracing::debug!(%remote, %ufrag, "webrtc-direct: 发起出站连接");

    let se = base_setting_engine(&ufrag, remote);
    // dialer **不走 mux**：它没有需要复用的固定端口，独占一个临时端口即可。
    // 官方实现强制 dialer 借用 listener 的 mux（没有 listener 就报 `NoListeners`），
    // 我们不需要那条限制——纯客户端（比如只拨不收的工具）照样能用。
    let (pc, mut events, inbound_dc) = build(se, None, remote, &ctx).await?;
    // 同 inbound 的守卫。这条路径上**拨号失败是常态**（拨到非 webrtc-direct 的端口、
    // certhash 不匹配、Noise 认证失败），而拨号会重试，泄漏按重试次数累积。
    let pc = ManagedPeerConnection::new(pc, ctx.runtime.clone());
    let noise_channel = create_noise_channel(pc.as_ref()).await?;

    // 先生成真 offer，再收下（构造的）answer——后者会启动 ICE。
    let offer = pc
        .create_offer(None)
        .await
        .map_err(|e| Error::Connection(format!("创建 offer 失败：{e}")))?;
    pc.set_local_description(offer)
        .await
        .map_err(|e| Error::Connection(format!("设置本地 offer 失败：{e}")))?;
    pc.set_remote_description(
        sdp::answer(remote, server_fingerprint, &ufrag, ctx.stream_config)
            .map_err(|e| Error::Connection(e.to_string()))?,
    )
    .await
    .map_err(|e| Error::Connection(format!("设置构造的 answer 失败：{e}")))?;

    with_timeout(await_connected(&mut events)).await?;

    with_timeout(await_channel_open(&noise_channel)).await?;

    let client_fingerprint = ctx.certificate.fingerprint();
    let noise_channel_id = noise_channel.id();
    let stream = wrap_noise_channel(noise_channel, &ctx);
    let (peer, stream_config) = with_timeout(noise::outbound_with_message_size(
        ctx.id_keys.clone(),
        stream,
        server_fingerprint,
        client_fingerprint,
        ctx.stream_config,
    ))
    .await?;

    tracing::debug!(%remote, %peer, "webrtc-direct: 出站连接就绪");
    Ok((
        peer,
        finish(pc, inbound_dc, stream_config, peer, noise_channel_id),
    ))
}

/// 两个方向共用的 `SettingEngine` 配置。
fn base_setting_engine(ufrag: &str, remote: SocketAddr) -> SettingEngine {
    let mut se = SettingEngine::default();

    // ufrag 与 pwd 同值——spec 如此规定，两端本地构造 SDP 时都按这条填。
    se.set_ice_credentials(ufrag.to_owned(), ufrag.to_owned());

    // **必须关掉 mDNS。** 开着的话 `PeerConnection` 会额外绑一个 `0.0.0.0:5353`
    // 多播 socket，而它同样走 `Runtime::wrap_udp_socket`——于是 mux 支路会被交出去
    // 两次。更糟的是 driver 对 mDNS socket 用 `AsyncUdpSocket::local_addr()`（我们
    // 返回的是共享监听端口）建索引，而对 ICE socket 用 bind 时的临时端口，于是同一
    // 批入站包会被随机打上两种 local_addr 标签，其中一半永远匹配不上 local candidate。
    //
    // 症状极隐蔽：ICE 只在 warn 级别刷
    // `Discarded message, not a valid local candidate`，握手静默超时。**实测踩过。**
    //
    // direct 模式本来也不需要 mDNS：地址是确定性构造的，没有要发现的东西。
    se.set_multicast_dns_mode(MulticastDnsMode::Disabled);

    // 只用与对端同族的网络类型。不限定的话 ICE agent 可能挑一个 IPv6 本地候选去
    // 连 IPv4 目标（官方注释里点名了这个坑）。
    se.set_network_types(vec![match remote {
        SocketAddr::V4(_) => NetworkType::Udp4,
        SocketAddr::V6(_) => NetworkType::Udp6,
    }]);

    se
}

/// [`build`] 的产物：连接本体 + 两条事件轨。
type Built = (
    Arc<dyn PeerConnection>,
    mpsc::UnboundedReceiver<StateEvent>,
    mpsc::UnboundedReceiver<Arc<dyn DataChannel>>,
);

/// 构造 `PeerConnection`。
///
/// `socket` 为 `Some` 时（listener 侧）把它的 socket 换成 mux 支路；为 `None` 时
/// （dialer 侧）让 `PeerConnection` 自己绑一个临时端口。
async fn build(
    se: SettingEngine,
    socket: Option<Arc<MuxedSocket>>,
    remote: SocketAddr,
    ctx: &Context,
) -> Result<Built, Error> {
    let mut media_engine = MediaEngine::default();
    media_engine
        .register_default_codecs()
        .map_err(|e| Error::Connection(format!("注册编解码器失败：{e}")))?;
    let registry = register_default_interceptors(Registry::new(), &mut media_engine)
        .map_err(|e| Error::Connection(format!("注册 interceptor 失败：{e}")))?;

    let config = RTCConfigurationBuilder::new()
        // direct 模式**不配 STUN**：一端已有可达地址，另一端只需一个 host candidate。
        // 配了反而会拖慢建连（要等 srflx 收集）。
        .with_certificates(vec![ctx.certificate.to_rtc()])
        .build();

    let (state_tx, state_rx) = mpsc::unbounded();
    let (dc_tx, dc_rx) = mpsc::unbounded();

    // listener 侧：socket 被 `MuxedRuntime` 换成 mux 支路，`PeerConnection` 自己绑的
    // 那个端口拿到手就丢弃，所以绑回环最不打扰系统（不占公网端口、不弹防火墙提示）。
    // 详见 `udp_mux::MuxedRuntime` 关于 four-tuple `local_addr` 的说明。
    //
    // dialer 侧：真的要用这个 socket，按本机网卡绑。
    //
    // ⚠️ **listener 侧那个地址列表必须恰好一个元素。** `PeerConnection` 按列表里每个
    // 地址绑一个 socket，而 `MuxedRuntime` 会把**同一个** `Arc<MuxedSocket>` 发回给每次
    // 调用；两个 socket 就意味着两个 recv future 抢同一个 mpsc `Receiver`，而它的
    // `AtomicWaker` 只记得最后注册的那个——另一个永远醒不来，包也会被打上错误的
    // `local_addr`。症状正是 `MuxedRuntime` 上面记的那种静默握手超时。
    // 这条前提由 `MuxedSocket` 那侧的 `debug_assert` 兜底。
    let (runtime, bind_addrs) = match socket {
        Some(socket) => (
            MuxedRuntime::wrap(ctx.runtime.clone(), socket),
            vec!["127.0.0.1:0".to_string()],
        ),
        None => (ctx.runtime.clone(), dialer_bind_addrs(remote)),
    };

    let pc = PeerConnectionBuilder::new()
        .with_configuration(config)
        .with_media_engine(media_engine)
        .with_interceptor_registry(registry)
        .with_setting_engine(se)
        .with_handler(Arc::new(Forwarder {
            state: state_tx,
            dc: dc_tx,
        }))
        .with_runtime(runtime)
        .with_udp_addrs(bind_addrs)
        // 窗口按**本端声明的**消息尺寸推导（`DirectConfig::with_max_message_size`，
        // 与打洞模式的默认值不同）。**不是协商值**——协商要等 Noise 握手，而它发生在
        // `build()` 之后（见下面 `inbound_handshake` / `outbound_handshake`），
        // 且窗口在 `PeerConnection` 建成后改不了。这条局限写在 `sctp_receive_buffer` 的
        // 文档第 3 点，别在这里把它说成协商值。
        .with_sctp_receive_buffer_size(sctp_receive_buffer(ctx.stream_config))
        .with_data_channel_send_buffer_limit(SEND_BUFFER_LIMIT)
        .build()
        .await
        .map_err(|e| Error::Connection(format!("构造 PeerConnection 失败：{e}")))?;

    Ok((Arc::new(pc), state_rx, dc_rx))
}

/// dialer 侧 ICE 要绑的本地地址。
///
/// 与打洞模式的同名逻辑不同，这里传 `0.0.0.0` 其实**无害**——服务端是 ICE-lite，
/// 它按收包的四元组回，从不使用我们通告的 candidate。但仍然枚举网卡：多网卡机器上
/// 让 ICE agent 自己挑一条能通的源地址，比赌通配绑定的默认出口更可靠。
///
/// **回环不能排除**：拨 `127.0.0.1` 的服务端（同机、集成测试）时，只有绑在回环上的
/// 候选才通得了——从 en0 的地址发往 `127.0.0.1` 会被内核路由挡掉。
fn dialer_bind_addrs(remote: SocketAddr) -> Vec<String> {
    let want_v4 = remote.is_ipv4();
    let addrs: Vec<String> = if_addrs::get_if_addrs()
        .map(|ifaces| {
            ifaces
                .into_iter()
                // 只取与目标同族的地址：`set_network_types` 已经限定了族，
                // 绑上另一族的地址只会白白多出永远配不上的候选。
                .filter(|i| i.addr.ip().is_ipv4() == want_v4)
                .map(|i| format!("{}:0", i.addr.ip()))
                .collect()
        })
        .unwrap_or_default();

    if addrs.is_empty() {
        tracing::warn!(%remote, "未能枚举出同族网卡地址，webrtc-direct 拨号退回通配绑定");
        return vec![if want_v4 { "0.0.0.0:0" } else { "[::]:0" }.to_string()];
    }
    addrs
}

/// 建 Noise 握手用的通道。
///
/// `negotiated: Some(0)` —— **两端都必须建**，且都用 id 0。它不经 SDP 通告
/// （out-of-band），所以对端不会在 `on_data_channel` 里看到它，也就不会被误当成
/// 一条业务子流交给上层。
async fn create_noise_channel(pc: &dyn PeerConnection) -> Result<Arc<dyn DataChannel>, Error> {
    pc.create_data_channel(
        "",
        Some(RTCDataChannelInit {
            negotiated: Some(0),
            ordered: true,
            ..Default::default()
        }),
    )
    .await
    .map_err(|e| Error::Connection(format!("创建 Noise 通道失败：{e}")))
}

/// 等 Noise 通道真正 `Open`，见 [`data_channel::await_open`] 的时序说明。
async fn await_channel_open(dc: &Arc<dyn DataChannel>) -> Result<(), Error> {
    data_channel::await_open(dc)
        .await
        .map_err(|e| Error::Connection(format!("等待 Noise 通道打开失败：{e}")))
}

/// 把 Noise 通道包成 libp2p 的字节流。
///
/// drop listener 直接丢弃：握手期间没有「子流被取消」这回事，握手完成后这条通道
/// 也不再使用。
fn wrap_noise_channel(
    dc: Arc<dyn DataChannel>,
    ctx: &Context,
) -> libp2p_webrtc_utils::Stream<crate::backend::native::data_channel::PollDataChannel> {
    let (stream, drop_listener) = libp2p_webrtc_utils::Stream::with_config(
        crate::backend::native::data_channel::PollDataChannel::new(dc),
        ctx.stream_config,
    );
    drop(drop_listener);
    stream
}

/// 组装最终的 [`Connection`]。
///
/// `noise_channel_id` 要交给 muxer——那条通道是本端建的，会被 driver 回灌进
/// `on_data_channel`，不挡住就会被当成第一条入站子流交给上层（它握手完就关了，
/// 上层一读即 EOF）。
fn finish(
    pc: ManagedPeerConnection,
    inbound_dc: mpsc::UnboundedReceiver<Arc<dyn DataChannel>>,
    stream_config: StreamConfig,
    peer: PeerId,
    noise_channel_id: RTCDataChannelId,
) -> Connection {
    // 握手成功：把**守卫本身** move 给 muxer，由它继续保证关闭——而不是在这里解除守卫
    // 再交裸连接（那样中间会有一段没人负责关闭的窗口）。
    let muxer = Muxer::with_config(
        pc,
        inbound_dc,
        stream_config,
        std::iter::once(noise_channel_id).collect(),
    );
    Connection::new(peer, libp2p_core::muxing::StreamMuxerBox::new(muxer))
}

/// 给一个 future 套上握手超时。
///
/// 没有它，拨一个「端口开着但不是 webrtc-direct」的地址会永久挂起——ICE 收不到
/// 任何应答，而 `PeerConnection` 的 failed 状态要等它内部的超时（可能很久）。
async fn with_timeout<T>(
    fut: impl Future<Output = Result<T, impl std::fmt::Display>>,
) -> Result<T, Error> {
    match futures::future::select(std::pin::pin!(fut), Delay::new(HANDSHAKE_TIMEOUT)).await {
        Either::Left((Ok(v), _)) => Ok(v),
        Either::Left((Err(e), _)) => Err(Error::Connection(e.to_string())),
        Either::Right(((), _)) => Err(Error::Connection(format!(
            "握手超过 {} 秒未完成",
            HANDSHAKE_TIMEOUT.as_secs()
        ))),
    }
}

/// DTLS 握手完成后，取对端证书的 SHA-256 指纹。
///
/// 服务端必须拿到**拨号端**的指纹才能算出与对面一致的 Noise prologue，而 direct 模式下
/// 它收不到真 offer（自己合成、填 `Fingerprint::FF`），该值只存在于 DTLS 握手里。
///
/// 0.20 没有 0.17 那个 `sctp().transport().get_remote_certificate()`，只能从统计报告里反查，
/// 而报告的组织方式让这件事**很容易做反**：它给每一侧各放一条 `Certificate` 条目，条目本身
/// 不说自己属于哪边，只有 `Transport` 条目的 `remote_certificate_id` 能把两者分开。取错了不
/// 报错——prologue 会拿本端指纹跟本端比，Noise 随后失败，但错因完全看不出来。
///
/// 上游曾提议把这段封进 `PeerConnection`
/// （<https://github.com/webrtc-rs/webrtc/pull/828>），评审结论是不给 trait 加 W3C 之外的
/// 方法，故留在这里。
async fn remote_fingerprint(pc: &dyn PeerConnection) -> Result<Fingerprint, Error> {
    let report = pc
        .get_stats(std::time::Instant::now(), StatsSelector::None)
        .await;

    let remote_id = report
        .iter()
        .find_map(|entry| match entry {
            RTCStatsReportEntry::Transport(transport)
                if !transport.remote_certificate_id.is_empty() =>
            {
                Some(transport.remote_certificate_id.clone())
            }
            _ => None,
        })
        .ok_or_else(|| Error::Connection("DTLS 握手后统计报告里没有对端证书".into()))?;

    let value = report
        .iter()
        .find_map(|entry| match entry {
            RTCStatsReportEntry::Certificate(cert) if cert.stats.id == remote_id => {
                Some(cert.fingerprint.clone())
            }
            _ => None,
        })
        .ok_or_else(|| Error::Connection("DTLS 握手后仍拿不到对端证书指纹".into()))?;

    crate::protocol::addr::parse_sdp_fingerprint(&value)
        .ok_or_else(|| Error::Connection(format!("对端证书指纹格式无法解析：{value}")))
}

/// 把 `PeerConnection` 的回调转成 channel。
#[derive(Clone)]
struct Forwarder {
    state: mpsc::UnboundedSender<StateEvent>,
    dc: mpsc::UnboundedSender<Arc<dyn DataChannel>>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for Forwarder {
    async fn on_data_channel(&self, dc: Arc<dyn DataChannel>) {
        let _ = self.dc.unbounded_send(dc);
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        let event = match state {
            RTCPeerConnectionState::Connected => StateEvent::Connected,
            RTCPeerConnectionState::Failed => StateEvent::Failed,
            _ => return,
        };
        let _ = self.state.unbounded_send(event);
    }

    // direct 模式**不做 trickle ICE**：本地候选是确定性的、已经写进对端本地构造的
    // SDP 里，没有可发送的信道，也没有必要发。故不实现 `on_ice_candidate`。
}
