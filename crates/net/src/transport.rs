//! Swarm 组装：transport 栈按 target 静态构建。
//!
//! | target | transports |
//! |---|---|
//! | native  | TCP + QUIC + DNS + webrtc-direct + relay client |
//! | android | 同 native，但 DNS 换显式公共解析（见 build_swarm 的 cfg 分支）|
//! | wasm    | webrtc-direct + webrtc 打洞 + relay client（不能 listen 本地 socket）|
//!
//! **WebSocket 已整体移除**（2026-07-28）。它唯一的活是「同网浏览器直连桌面」，而
//! webrtc-direct 把这件事做得更好：不占 TCP 端口、私网公网同一条路径、且没有
//! 「circuit 地址被 ws transport 抢走」这类前缀误匹配问题（实测踩过，见
//! `webrtc_and_relay`）。移除后 Android 与桌面的 transport 栈也终于一致了。
//!
//! 说明：
//! - SwarmBuilder 是编译期类型状态链，无法按运行时配置增删 transport；
//!   「启不启用」用 behaviour 侧 `Toggle`（relay）或「不 listen」（tcp）表达，
//!   builder 链本身保持单条不分叉（旧栈同款手法）。
//! - **relay client 由本模块自己装配**，不走 `with_relay_client`——那个方法会把 relay
//!   排到 `or_transport` 最前，抢走 WebRTC 打洞地址（原委见 `webrtc_and_relay`）。
//!   双 target 同款：浏览器经 circuit 被动接收连接也靠它。
//! - webrtc-direct（native 侧 server/dialer）在 M2 经 `with_other_transport` 接入。
//! - webrtc-p2p（打洞）默认关闭，`Builder::webrtc_p2p(..)` 启用；双 target 同款接法。

use libp2p::Swarm;
use libp2p::identity::Keypair;
use std::num::NonZeroUsize;

use crate::behaviour::Behaviour;
use crate::config::EndpointConfig;

/// Swarm 组装失败。
#[derive(Debug, thiserror::Error)]
#[error("failed to build swarm: {0}")]
pub struct BuildSwarmError(String);

/// 所有端声明的单条编码 DataChannel 消息上限。
///
/// WebRTC transport 在 Noise 认证后自动协商双方较小值；统一声明 8 KiB 可使浏览器、
/// 桌面、移动端及旧端回退路径都不会发送超出浏览器安全上限的帧。
const WEBRTC_MAX_MESSAGE_SIZE: NonZeroUsize =
    NonZeroUsize::new(8 * 1024).expect("8 KiB is non-zero");

/// 浏览器 DataChannel 回调尚未被 Rust 消费时允许累计的最大字节数。
///
/// 它是本地资源上限，不参与对端协商；须大于单条消息上限，避免多个合法回调在 wasm
/// 事件循环中连续到达时被误判为远端过载。
#[cfg(wasm_browser)]
const WEBRTC_MAX_READ_BUFFER_SIZE: NonZeroUsize =
    NonZeroUsize::new(256 * 1024).expect("256 KiB is non-zero");

/// 按配置创建 WebRTC 打洞传输的**两个平面**，未启用时返回一对 `None`。
///
/// ⚠️ 两者必须来自同一次 [`webrtc_p2p::new`] 且注册进同一个 Swarm——transport 的 dial
/// 只是把请求转交 behaviour（信令要在一条已建立的 relay 连接上开流，那是 behaviour 的
/// 能力）。只注册其一时 dial 会以 `BehaviourDetached` 快速失败。
///
/// `factory` 由调用方按 target 注入：native = `webrtc-rs`，wasm = 浏览器
/// `RTCPeerConnection`，两者毫无共同点。
fn build_webrtc_p2p(
    config: &EndpointConfig,
    factory: webrtc_p2p::Factory,
) -> (Option<webrtc_p2p::Transport>, Option<webrtc_p2p::Behaviour>) {
    let Some(cfg) = config.webrtc_p2p.as_ref() else {
        return (None, None);
    };
    let (transport, behaviour) = webrtc_p2p::new(
        webrtc_p2p::Config::default()
            .with_stun_servers(cfg.stun_servers.iter().cloned())
            .with_signaling_timeout(cfg.signaling_timeout),
        factory,
    );
    (Some(transport), Some(behaviour))
}

/// 造「WebRTC 打洞 + relay client」这条 transport，并返回配套的 relay behaviour。
///
/// 双 target 共用——**relay 的 upgrade 链只此一处**，否则两个 cfg 分支各写一遍，
/// 将来改 noise / yamux 配置就得记得两边同步。
///
/// # 为什么不用 `SwarmBuilder::with_relay_client`
///
/// 打洞地址 `<relay>/p2p-circuit/webrtc/p2p/<target>` 含 `/p2p-circuit` 段，而 relay
/// client transport 的 `parse_relayed_multiaddr` 只要求有 circuit 段——**circuit 之后的
/// `/webrtc` 被塞进 `dst_addr` 而不报错**，于是它照单全收。
///
/// 谁先拿到取决于 `or_transport` 的顺序，而 `with_relay_client` 内部写死的是
/// `relay_transport.or_transport(已有链)`——relay **永远在最前**，用
/// `with_other_transport` 无论注册多少次都抢不过它。
///
/// 后果极隐蔽（浏览器实测）：`listen_on` / `dial` 全被 relay 接走，reservation 和中转
/// 都正常工作，**打洞路径一次都没被调用过**，且没有任何报错。
///
/// # 注册位置的约束
///
/// 本函数的产物必须排在**任何会按前缀吞掉 circuit 地址的 transport 之前**。历史上
/// WebSocket 就是这样一个：circuit 地址形如 `/ip4/…/tcp/…/ws/p2p/<relay>/p2p-circuit/…`，
/// 它只认前缀就照单全收、无视 circuit 段，于是真的连上 relay，然后 Swarm 发现对端是
/// relay 而非 target，报 `WrongPeerId`（实测踩过）。WebSocket 已于 2026-07-28 整体移除，
/// 现存 transport（tcp/quic/webrtc-direct）都不认 circuit 地址，故顺序压力已解除——
/// 但**再引入新 transport 时要重新审这一条**。
///
/// js-libp2p 没有这个问题——它的 circuit transport filter 显式排除了带 `/webrtc` 的地址。
/// rust-libp2p 缺这道排除，因为上游还没有 private-to-private 的 WebRTC 传输。
#[expect(
    clippy::type_complexity,
    reason = "libp2p 的 transport 组合类型天然如此"
)]
fn webrtc_and_relay(
    key: &Keypair,
    webrtc: Option<webrtc_p2p::Transport>,
    relay_client: bool,
) -> Result<
    (
        libp2p::core::transport::Boxed<(libp2p::PeerId, libp2p::core::muxing::StreamMuxerBox)>,
        Option<libp2p::relay::client::Behaviour>,
    ),
    libp2p::noise::Error,
> {
    use libp2p::core::muxing::StreamMuxerBox;
    use libp2p::core::transport::{OptionalTransport, Transport as _};
    use libp2p::core::upgrade::Version;
    use libp2p::{noise, yamux};

    let (relay_transport, relay_behaviour) = libp2p::relay::client::new(key.public().to_peer_id());
    let relay = relay_transport
        .upgrade(Version::V1Lazy)
        .authenticate(noise::Config::new(key)?)
        .multiplex(yamux::Config::default())
        .map(|(peer, conn), _| (peer, StreamMuxerBox::new(conn)));

    let webrtc = match webrtc {
        Some(t) => {
            OptionalTransport::some(t.map(|(peer, conn), _| (peer, StreamMuxerBox::new(conn))))
        }
        None => OptionalTransport::none(),
    };

    // ⚠️ 顺序即语义：webrtc-p2p 在前，它只认带 `/webrtc` 的地址，其余原样落给 relay。
    let transport = webrtc
        .or_transport(relay)
        .map(|either, _| either.into_inner())
        .boxed();
    Ok((transport, relay_client.then_some(relay_behaviour)))
}

#[cfg(not(wasm_browser))]
pub(crate) fn build_swarm(
    keypair: Keypair,
    config: &EndpointConfig,
) -> Result<Swarm<Behaviour>, BuildSwarmError> {
    use libp2p::core::muxing::StreamMuxerBox;
    use libp2p::core::transport::Transport as _;
    use libp2p::{SwarmBuilder, noise, tcp, yamux};

    let err = |e: &dyn std::fmt::Display| BuildSwarmError(e.to_string());

    let (webrtc_p2p_transport, webrtc_p2p_behaviour) = build_webrtc_p2p(
        config,
        webrtc_p2p::backend::native::NativeBackend::factory(),
    );
    // relay client 的 behaviour 在 transport 构造闭包里产出，经此回传给 with_behaviour。
    let mut relay_behaviour = None;

    let builder = SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )
        .map_err(|e| err(&e))?
        .with_quic()
        // webrtc-direct：浏览器拨公网/私网裸 IP 的入口（certhash 免域名免 CA，
        // spike/webrtc-direct-https 实证）。是否 listen 由地址决定（/webrtc-direct）。
        .with_other_transport(|key| {
            let cert = match &config.webrtc_cert_pem {
                Some(pem) => libp2p_webrtc::tokio::Certificate::from_pem(pem)
                    .map_err(|e| std::io::Error::other(e.to_string()))?,
                None => {
                    tracing::warn!(
                        "webrtc-direct using ephemeral certificate; \
                         certhash addresses will not survive restarts"
                    );
                    libp2p_webrtc::tokio::Certificate::generate(&mut rand::thread_rng())
                        .map_err(|e| std::io::Error::other(e.to_string()))?
                }
            };
            Ok(libp2p_webrtc::tokio::Transport::new(key.clone(), cert)
                .with_max_message_size(WEBRTC_MAX_MESSAGE_SIZE)
                .map(|(peer, conn), _| (peer, StreamMuxerBox::new(conn))))
        })
        .map_err(|e| err(&e))?
        // WebRTC 打洞 + relay client 合成一条，**顺序由我们定**（见 `webrtc_and_relay`）。
        .with_other_transport(|key| {
            let (transport, relay) =
                webrtc_and_relay(key, webrtc_p2p_transport, config.relay_client)?;
            relay_behaviour = relay;
            Ok(transport)
        })
        .map_err(|e| err(&e))?;

    // Android：hickory 的 system_conf 经 JNI（ndk-context）读系统 DNS，RN 宿主没有
    // 初始化入口 → start 报 "android context was not initialized"。换成显式公共 DNS。
    // 排障细节见 dev-notes/knowledge/net-kernel.md 坑 7。
    //（曾经的第二处炸点 `with_websocket` 随 WebSocket 整体移除而消失。）
    #[cfg(target_os = "android")]
    let builder =
        builder.with_dns_config(android_dns_config(), libp2p::dns::ResolverOpts::default());

    #[cfg(not(target_os = "android"))]
    let builder = builder.with_dns().map_err(|e| err(&e))?;

    let swarm = builder
        // 单参数 with_behaviour = 跳过 builder 的 relay phase（relay transport 已在上面
        // 自行组装）。多传一个 relay_client 参数就会走回那条把 relay 排最前的路。
        .with_behaviour(|key| Behaviour::new(key, relay_behaviour, webrtc_p2p_behaviour, config))
        .map_err(|e| err(&e))?
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(config.idle_timeout))
        .build();

    Ok(swarm)
}

/// Android 公共 DNS 兜底列表（政策位，换供应商只改这里）：
/// AliDNS / DNSPod / Cloudflare / Google。只有 relay/对端的 /dns4 地址会经它解析，
/// bootstrap 走裸 IP 不受影响。
#[cfg(target_os = "android")]
const ANDROID_FALLBACK_DNS: &[std::net::Ipv4Addr] = &[
    std::net::Ipv4Addr::new(223, 5, 5, 5),
    std::net::Ipv4Addr::new(119, 29, 29, 29),
    std::net::Ipv4Addr::new(1, 1, 1, 1),
    std::net::Ipv4Addr::new(8, 8, 8, 8),
];

#[cfg(target_os = "android")]
fn android_dns_config() -> libp2p::dns::ResolverConfig {
    use hickory_resolver::config::NameServerConfig;

    let servers = ANDROID_FALLBACK_DNS
        .iter()
        .map(|&ip| NameServerConfig::udp_and_tcp(ip.into()))
        .collect();
    libp2p::dns::ResolverConfig::from_parts(None, vec![], servers)
}

#[cfg(wasm_browser)]
pub(crate) fn build_swarm(
    keypair: Keypair,
    config: &EndpointConfig,
) -> Result<Swarm<Behaviour>, BuildSwarmError> {
    use libp2p::core::muxing::StreamMuxerBox;
    use libp2p::core::transport::Transport as _;
    use libp2p::{SwarmBuilder, webrtc_websys};

    let err = |e: &dyn std::fmt::Display| BuildSwarmError(e.to_string());

    let (webrtc_p2p_transport, webrtc_p2p_behaviour) =
        build_webrtc_p2p(config, webrtc_p2p::backend::wasm::WasmBackend::factory());
    // 同 native：relay behaviour 在 transport 构造闭包里产出，经此回传。
    let mut relay_behaviour = None;

    let swarm = SwarmBuilder::with_existing_identity(keypair)
        .with_wasm_bindgen()
        // ⚠️ **必须排在 ws/webrtc-websys 之前**：circuit 地址形如
        // `/ip4/…/tcp/…/ws/p2p/<relay>/p2p-circuit/p2p/<target>`，而 websocket transport
        // 只认前缀 `/ip4/…/tcp/…/ws` 就照单全收，无视后面的 circuit 段——它会真的连上
        // relay，然后 Swarm 发现对端是 relay 而非 target，报 `WrongPeerId`（实测）。
        // 官方 `with_relay_client` 把 relay 塞在最前正是为此；我们自己组装就得自己保证。
        .with_other_transport(|key| {
            let (transport, relay) =
                webrtc_and_relay(key, webrtc_p2p_transport, config.relay_client)?;
            relay_behaviour = relay;
            Ok(transport)
        })
        .map_err(|e| err(&e))?
        .with_other_transport(|key| {
            // webrtc-direct：拨已可达的对端（同网桌面 / 公网 relay）。自带 noise
            // 与分帧，不需要 upgrade 链。
            Ok(webrtc_websys::Transport::new(
                webrtc_websys::Config::new(key)
                    .with_max_message_size(WEBRTC_MAX_MESSAGE_SIZE)
                    .with_max_read_buffer_size(WEBRTC_MAX_READ_BUFFER_SIZE),
            )
            .map(|(p, c), _| (p, StreamMuxerBox::new(c))))
        })
        .map_err(|e| err(&e))?
        // 单参数 = 跳过 builder 的 relay phase（原委见 native 分支）。
        .with_behaviour(|key| Behaviour::new(key, relay_behaviour, webrtc_p2p_behaviour, config))
        .map_err(|e| err(&e))?
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(config.idle_timeout))
        .build();

    Ok(swarm)
}
