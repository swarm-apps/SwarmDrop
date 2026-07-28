//! Swarm 组装：transport 栈按 target 静态构建。
//!
//! | target | transports |
//! |---|---|
//! | native  | TCP + QUIC + DNS + WebSocket(listen 给浏览器) + relay client |
//! | android | 同 native，但 DNS 换显式公共解析、去 WebSocket（见 build_swarm 的 cfg 分支）|
//! | wasm    | webrtc-websys + websocket-websys + relay client（不能 listen 本地 socket）|
//!
//! 说明：
//! - SwarmBuilder 是编译期类型状态链，无法按运行时配置增删 transport；
//!   「启不启用」用 behaviour 侧 `Toggle`（relay）或「不 listen」（tcp/ws）表达，
//!   builder 链本身保持单条不分叉（旧栈同款手法）。
//! - relay phase 无 target gate（`cfg(feature = "relay")` only）——wasm 也走
//!   `with_relay_client`，浏览器经 circuit 被动接收连接靠它。
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

#[cfg(not(wasm_browser))]
pub(crate) async fn build_swarm(
    keypair: Keypair,
    config: &EndpointConfig,
) -> Result<Swarm<Behaviour>, BuildSwarmError> {
    use libp2p::core::muxing::StreamMuxerBox;
    use libp2p::core::transport::{OptionalTransport, Transport as _};
    use libp2p::{SwarmBuilder, noise, tcp, yamux};

    let err = |e: &dyn std::fmt::Display| BuildSwarmError(e.to_string());

    let (webrtc_p2p_transport, webrtc_p2p_behaviour) = build_webrtc_p2p(
        config,
        webrtc_p2p::backend::native::NativeBackend::factory(),
    );

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
        // WebRTC 打洞。**必须排在 `with_relay_client` 之前**：它拨的
        // `<relay>/p2p-circuit/webrtc/p2p/<target>` 含 `/p2p-circuit` 段，relay client
        // transport 同样认这类地址；`with_relay_client` 内部是
        // `已有 transport.or_transport(relay)`，先注册者优先，晚了地址就被 relay 抢去
        // 当普通中转拨号——链路能通但永远打不了洞，且无任何报错。
        .with_other_transport(|_| {
            // 错误类型无法从闭包体推断（这里没有任何 `?` 触发 From 转换），显式标注成
            // `TryIntoTransport` 唯一认的那个 Result 形态。
            type TransportError = Box<dyn std::error::Error + Send + Sync>;
            // `OptionalTransport` 只有 `From<T>`，没有 `From<Option<T>>`——顺手写
            // `.from(opt)` 会包成 `OptionalTransport<Option<_>>`，那不是 Transport。
            Ok::<_, TransportError>(match webrtc_p2p_transport {
                Some(t) => OptionalTransport::some(
                    t.map(|(peer, conn), _| (peer, StreamMuxerBox::new(conn))),
                ),
                None => OptionalTransport::none(),
            })
        })
        .map_err(|e| err(&e))?;

    // Android：hickory 的 system_conf 经 JNI（ndk-context）读系统 DNS，RN 宿主没有
    // 初始化入口 → start 报 "android context was not initialized"。炸点有两处，
    // 排障细节见 dev-notes/knowledge/net-kernel.md 坑 7：
    // 1. with_dns() → 换 with_dns_config 显式公共 DNS；
    // 2. with_websocket() → 跳过——其宏展开硬编码 Transport::system，不吃
    //    with_dns_config。代价：Android 端 /ws、/wss 地址完全不可拨（当前无消费方）。
    #[cfg(target_os = "android")]
    let builder =
        builder.with_dns_config(android_dns_config(), libp2p::dns::ResolverOpts::default());

    #[cfg(not(target_os = "android"))]
    let builder = builder
        .with_dns()
        .map_err(|e| err(&e))?
        // WebSocket listener：LanHelper 给浏览器的入口（ws:// 私有 IP 豁免
        // mixed content，spike 实证）。是否真的 listen 由 listen 地址决定。
        .with_websocket(noise::Config::new, yamux::Config::default)
        .await
        .map_err(|e| err(&e))?;

    let swarm = builder
        .with_relay_client(noise::Config::new, yamux::Config::default)
        .map_err(|e| err(&e))?
        .with_behaviour(|key, relay_client| {
            Behaviour::new(
                key,
                config.relay_client.then_some(relay_client),
                webrtc_p2p_behaviour,
                config,
            )
        })
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
pub(crate) async fn build_swarm(
    keypair: Keypair,
    config: &EndpointConfig,
) -> Result<Swarm<Behaviour>, BuildSwarmError> {
    use libp2p::core::muxing::StreamMuxerBox;
    use libp2p::core::transport::{OptionalTransport, Transport as _};
    use libp2p::core::upgrade::Version;
    use libp2p::{SwarmBuilder, noise, webrtc_websys, websocket_websys, yamux};

    let err = |e: &dyn std::fmt::Display| BuildSwarmError(e.to_string());

    let (webrtc_p2p_transport, webrtc_p2p_behaviour) =
        build_webrtc_p2p(config, webrtc_p2p::backend::wasm::WasmBackend::factory());

    let swarm = SwarmBuilder::with_existing_identity(keypair)
        .with_wasm_bindgen()
        .with_other_transport(|key| {
            // websocket-websys 没有便捷方法，手动 upgrade/authenticate/multiplex
            // （照 rust-libp2p/interop-tests/src/arch.rs 的官方组合；
            //  spike/webrtc-direct-https 实测通过）。Window/Worker 双环境可用。
            let ws = websocket_websys::Transport::default()
                .upgrade(Version::V1Lazy)
                .authenticate(noise::Config::new(key)?)
                .multiplex(yamux::Config::default())
                .map(|(p, c), _| (p, StreamMuxerBox::new(c)));

            // Worker 环境（无 window）不装 webrtc-websys：它的 dial 在地址格式检查
            // **之前**就调 maybe_local_firefox()（内含 window().expect）——装了它，
            // 经 or_transport 拨任何地址（含 ws）都先进 webrtc 分支碰 window panic。
            // 实测坐实（2026-07-18 Worker 版基准），非只影响 webrtc 地址。
            if web_sys::window().is_none() {
                return Ok(ws.boxed());
            }

            // webrtc-websys 自带 noise + 分帧，不需要 upgrade 链。
            let webrtc = webrtc_websys::Transport::new(
                webrtc_websys::Config::new(key)
                    .with_max_message_size(WEBRTC_MAX_MESSAGE_SIZE)
                    .with_max_read_buffer_size(WEBRTC_MAX_READ_BUFFER_SIZE),
            )
            .map(|(p, c), _| (p, StreamMuxerBox::new(c)));

            // or_transport 两道坎（E0271）：两侧先各自 map 成 StreamMuxerBox；
            // 摊平后 Output 仍是 future::Either，需 into_inner() 再塌缩一次。
            Ok(webrtc
                .or_transport(ws)
                .map(|either, _| either.into_inner())
                .boxed())
        })
        .map_err(|e| err(&e))?
        // 顺序同 native：必须在 `with_relay_client` 之前，否则 `/p2p-circuit/webrtc/…`
        // 地址会被 relay client transport 抢去当普通中转（详见 native 分支同处注释）。
        .with_other_transport(|_| {
            // 错误类型无法从闭包体推断（这里没有任何 `?` 触发 From 转换），显式标注成
            // `TryIntoTransport` 唯一认的那个 Result 形态。
            type TransportError = Box<dyn std::error::Error + Send + Sync>;
            // `OptionalTransport` 只有 `From<T>`，没有 `From<Option<T>>`——顺手写
            // `.from(opt)` 会包成 `OptionalTransport<Option<_>>`，那不是 Transport。
            Ok::<_, TransportError>(match webrtc_p2p_transport {
                Some(t) => OptionalTransport::some(
                    t.map(|(peer, conn), _| (peer, StreamMuxerBox::new(conn))),
                ),
                None => OptionalTransport::none(),
            })
        })
        .map_err(|e| err(&e))?
        .with_relay_client(noise::Config::new, yamux::Config::default)
        .map_err(|e| err(&e))?
        .with_behaviour(|key, relay_client| {
            Behaviour::new(
                key,
                config.relay_client.then_some(relay_client),
                webrtc_p2p_behaviour,
                config,
            )
        })
        .map_err(|e| err(&e))?
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(config.idle_timeout))
        .build();

    Ok(swarm)
}
