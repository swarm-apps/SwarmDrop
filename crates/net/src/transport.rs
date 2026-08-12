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
//! - webrtc-p2p（打洞）经 `Builder::webrtc_p2p(..)` 启用，双 target 同款接法；
//!   内核默认关，但 core 的组合根对三端一律开启（打洞要两端都支持）。

use libp2p::Swarm;
use libp2p::identity::Keypair;
use std::num::NonZeroUsize;
use swarmdrop_net_base::TransportKind;

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

/// libp2p 的 transport 组合类型天然很长，起个别名比到处 `#[expect(type_complexity)]` 干净。
///
/// 两个 target 都用得上（wasm 的 `build_webtransport` 也返回它），故不加 cfg 门控。
type BoxedTransport =
    libp2p::core::transport::Boxed<(libp2p::PeerId, libp2p::core::muxing::StreamMuxerBox)>;

/// 按配置创建 WebRTC 打洞传输的**两个平面**，未启用时返回一对 `None`。
///
/// ⚠️ 两者必须来自同一次 [`webrtc_p2p::new`] 且注册进同一个 Swarm——transport 的 dial
/// 只是把请求转交 behaviour（信令要在一条已建立的 relay 连接上开流，那是 behaviour 的
/// 能力）。只注册其一时 dial 会以 `BehaviourDetached` 快速失败。
///
/// `factory` 由调用方按 target 注入：native = `webrtc-rs`，wasm = 浏览器
/// `RTCPeerConnection`，两者毫无共同点。
/// 造 webrtc-p2p 的 transport（+ 打洞用的 behaviour）。
///
/// 它一个 transport 同时承载两种模式，按 multiaddr 分派：
///
/// - **direct（`/webrtc-direct`）始终启用**——它取代了官方 `libp2p-webrtc`，是浏览器
///   拨公网/同网裸 IP 的入口，不该跟着打洞开关走。
/// - **打洞（`/webrtc`）按 `config.webrtc_p2p`**。关闭时 behaviour 不注册，此时若真去拨
///   一个 `/webrtc` 地址会以 `BehaviourDetached` 快速失败——正是期望行为。
///
/// ⚠️ transport 与 behaviour 必须来自**同一次** [`webrtc_p2p::new`] 且注册进同一个
/// Swarm：打洞的 dial 只是把请求转交 behaviour（信令要在一条已建立的 relay 连接上
/// 开流，那是 behaviour 的能力）。
fn build_webrtc_p2p(
    keypair: &Keypair,
    config: &EndpointConfig,
    factory: webrtc_p2p::Factory,
) -> (webrtc_p2p::Transport, Option<webrtc_p2p::Behaviour>) {
    let hole_punch = config.webrtc_p2p.as_ref();

    let mut direct = webrtc_p2p::DirectConfig::new(keypair.clone())
        .with_max_message_size(WEBRTC_MAX_MESSAGE_SIZE);
    if let Some(pem) = config.webrtc_cert_pem.as_ref() {
        direct = direct.with_certificate_pem(pem);
    }

    let mut cfg = webrtc_p2p::Config::default().with_direct(direct);
    if let Some(hp) = hole_punch {
        cfg = cfg
            .with_stun_servers(hp.stun_servers.iter().cloned())
            .with_signaling_timeout(hp.signaling_timeout);
    }

    let (transport, behaviour) = webrtc_p2p::new(cfg, factory);
    (transport, hole_punch.is_some().then_some(behaviour))
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
fn webrtc_and_relay(
    key: &Keypair,
    webrtc: webrtc_p2p::Transport,
    webtransport: Option<BoxedTransport>,
    relay_client: bool,
) -> Result<(BoxedTransport, Option<libp2p::relay::client::Behaviour>), libp2p::noise::Error> {
    use libp2p::core::muxing::StreamMuxerBox;
    use libp2p::core::transport::Transport as _;
    use libp2p::core::upgrade::Version;
    use libp2p::{noise, yamux};

    let (relay_transport, relay_behaviour) = libp2p::relay::client::new(key.public().to_peer_id());
    let relay = relay_transport
        .upgrade(Version::V1Lazy)
        .authenticate(noise::Config::new(key)?)
        .multiplex(yamux::Config::default())
        .map(|(peer, conn), _| (peer, StreamMuxerBox::new(conn)));

    let webrtc = webrtc.map(|(peer, conn), _| (peer, StreamMuxerBox::new(conn)));

    // ⚠️ 顺序即语义：webrtc-p2p 在前，它只认 `/webrtc` 与 `/webrtc-direct` 两种地址，
    // 其余原样落给 relay。
    let transport = webrtc
        .or_transport(relay)
        .map(|either, _| either.into_inner())
        .boxed();

    // WebTransport 排最前。它只认含 `/webtransport` 段的地址，不会误吃别的。
    //
    // ⚠️ 反过来 **libp2p-quic 也不会吃它**：`multiaddr_to_socketaddr` 对 `/quic-v1`
    // 之后的任何非 `/p2p` 段一律 `return None`（上游 `transports/quic/src/transport.rs`）。
    // 这条判据是「WebTransport 地址不必排在 quic 之前」的全部依据 ——
    // **升 libp2p rev 时要重新确认它**，破了的话表现是 WebTransport 地址被 quic 认领、
    // 然后永远拨不通，且没有任何错误指向真正的原因。
    let transport = match webtransport {
        Some(wt) => wt
            .or_transport(transport)
            .map(|either, _| either.into_inner())
            .boxed(),
        None => transport,
    };
    Ok((transport, relay_client.then_some(relay_behaviour)))
}

/// 装配 WebTransport（native）。
///
/// 未注入证书端口 = 不启用，返回 `None` —— 于是 `/quic-v1/webtransport` 地址走到
/// `MultiaddrNotSupported` 快速失败，而不是挂着等超时。
#[cfg(not(wasm_browser))]
fn build_webtransport(
    key: &Keypair,
    config: &EndpointConfig,
) -> Result<Option<BoxedTransport>, Box<dyn std::error::Error + Send + Sync>> {
    use libp2p::core::muxing::StreamMuxerBox;
    use libp2p::core::transport::Transport as _;

    let Some(webtransport) = config.webtransport.as_ref() else {
        return Ok(None);
    };

    let mut cfg = webtransport_p2p::Config::new(key.clone());
    // 纯拨号方没有服务端证书要存 —— 见 `WebTransportConfig` 的文档。
    if let Some(store) = webtransport.store() {
        cfg = cfg.with_certificate_store(std::sync::Arc::clone(store));
    }
    let transport = webtransport_p2p::Transport::new(cfg)?;

    Ok(Some(
        transport
            .map(|(peer, muxer), _| (peer, StreamMuxerBox::new(muxer)))
            .boxed(),
    ))
}

/// 当前浏览器有没有 `WebTransport`。
///
/// **这是本仓第一个「浏览器可能没有」的传输** —— WebRTC 在所有目标浏览器上都在，所以此前
/// 没踩到这一类。Safari 直到 18.2、Firefox 直到 114 才有它，老 WebView 至今没有。
///
/// 不探测的后果正是 [`supported_transports`] 文档里那条禁忌：**多报**。用户在老浏览器上
/// 粘一条 `/quic-v1/webtransport` 引导节点，校验会通过、UI 会说「本端支持 webtransport」，
/// 而实际拨号在 `Connection::new` 处报的是 `TransportError::Other`（**不是**
/// `MultiaddrNotSupported`），因此也不会 fall through 给别的 transport —— 只留一句 JS 异常。
#[cfg(wasm_browser)]
fn browser_supports_webtransport() -> bool {
    js_sys::Reflect::has(
        &js_sys::global(),
        &wasm_bindgen::JsValue::from_str("WebTransport"),
    )
    .unwrap_or(false)
}

/// 装配 WebTransport（浏览器）。
///
/// 上游现成的 `libp2p-webtransport-websys`，**只能拨号** —— 浏览器起不了监听。
/// 无需配置也无开关，但要**探测浏览器有没有这个 API**：装配与能力申报必须用同一个判据，
/// 否则又变成多报或少报。
#[cfg(wasm_browser)]
fn build_webtransport(key: &Keypair) -> Option<BoxedTransport> {
    use libp2p::core::muxing::StreamMuxerBox;
    use libp2p::core::transport::Transport as _;

    if !browser_supports_webtransport() {
        tracing::info!("当前浏览器没有 WebTransport API，跳过该传输");
        return None;
    }

    Some(
        libp2p_webtransport_websys::Transport::new(libp2p_webtransport_websys::Config::new(key))
            .map(|(peer, conn), _| (peer, StreamMuxerBox::new(conn)))
            .boxed(),
    )
}

/// 本 target 实际装配的可拨传输种类。
///
/// **它必须与本模块两个 `build_swarm` 分支同步**——这是「这条地址本端拨得动吗」的唯一
/// 判据（`Endpoint::supported_transports`）。多报一种会让用户配下一条永远连不上的引导
/// 节点、且没有任何错误提示；少报一种会当场拒掉合法地址。所以它跟组装代码住同一个文件，
/// 加/删 transport 时不可能只改一边还看不见另一边。
///
/// circuit 地址不占独立变体：`Addr::transport()` 取的是外层中继段的传输
/// （`/ip4/../tcp/../p2p/<relay>/p2p-circuit/..` → `Tcp`），而那正是本机要拨的东西。
pub(crate) fn supported_transports(config: &EndpointConfig) -> Vec<TransportKind> {
    let mut kinds = Vec::with_capacity(4);

    // native 的 `.with_tcp()` + `.with_quic()`；浏览器起不了本地 socket，两者都没有。
    #[cfg(not(wasm_browser))]
    {
        kinds.push(TransportKind::Tcp);
        kinds.push(TransportKind::Quic);
    }

    // direct 双 target 恒装配，不跟打洞开关走（见 `build_webrtc_p2p` 的文档）。
    kinds.push(TransportKind::WebrtcDirect);

    // 打洞按配置：关闭时 behaviour 不注册，拨 `/webrtc` 会以 BehaviourDetached 快速失败。
    if config.webrtc_p2p.is_some() {
        kinds.push(TransportKind::Webrtc);
    }

    // WebTransport：浏览器**按 API 是否存在**（与装配同一个判据），native 跟着启用开关走。
    #[cfg(wasm_browser)]
    if browser_supports_webtransport() {
        kinds.push(TransportKind::Webtransport);
    }
    #[cfg(not(wasm_browser))]
    if config.webtransport.is_some() {
        kinds.push(TransportKind::Webtransport);
    }

    kinds
}

#[cfg(not(wasm_browser))]
pub(crate) fn build_swarm(
    keypair: Keypair,
    config: &EndpointConfig,
) -> Result<Swarm<Behaviour>, BuildSwarmError> {
    use libp2p::{SwarmBuilder, noise, tcp, yamux};

    let err = |e: &dyn std::fmt::Display| BuildSwarmError(e.to_string());

    let (webrtc_p2p_transport, webrtc_p2p_behaviour) = build_webrtc_p2p(
        &keypair,
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
        // WebRTC（打洞 + direct）+ relay client 合成一条，**顺序由我们定**
        // （见 `webrtc_and_relay`）。direct 那一半取代了官方 `libp2p-webrtc`——
        // 两者都基于 webrtc-rs，但版本不同（0.20 vs 0.17），并存等于把整套
        // ICE/DTLS/SCTP 编译两遍。
        .with_other_transport(|key| {
            let webtransport = build_webtransport(key, config)?;
            let (transport, relay) =
                webrtc_and_relay(key, webrtc_p2p_transport, webtransport, config.relay_client)?;
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
    use libp2p::SwarmBuilder;

    let err = |e: &dyn std::fmt::Display| BuildSwarmError(e.to_string());

    let (webrtc_p2p_transport, webrtc_p2p_behaviour) = build_webrtc_p2p(
        &keypair,
        config,
        webrtc_p2p::backend::wasm::WasmBackend::factory(),
    );
    // 同 native：relay behaviour 在 transport 构造闭包里产出，经此回传。
    let mut relay_behaviour = None;

    let swarm = SwarmBuilder::with_existing_identity(keypair)
        .with_wasm_bindgen()
        // 现在浏览器侧只剩这一条 transport：`webrtc-p2p` 同时认 `/webrtc`（打洞）与
        // `/webrtc-direct`，relay 排在它后面。官方 `webrtc-websys` 已于 2026-07-28
        // 移除——它的活由本 crate 的 direct 模式接了过来。
        //
        // ⚠️ 顺序仍是语义的一部分：再引入任何「按前缀吞地址」的 transport 时，必须
        // 排在这条**之后**（WebSocket 历史上就吞过 circuit 地址，实测报 `WrongPeerId`）。
        .with_other_transport(|key| {
            let (transport, relay) = webrtc_and_relay(
                key,
                webrtc_p2p_transport,
                build_webtransport(key),
                config.relay_client,
            )?;
            relay_behaviour = relay;
            Ok(transport)
        })
        .map_err(|e| err(&e))?
        // 单参数 = 跳过 builder 的 relay phase（原委见 native 分支）。
        .with_behaviour(|key| Behaviour::new(key, relay_behaviour, webrtc_p2p_behaviour, config))
        .map_err(|e| err(&e))?
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(config.idle_timeout))
        .build();

    Ok(swarm)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 护栏：清单与本 target 的 `build_swarm` 同步。
    ///
    /// 两条断言方向相反，都得在——只查「有」会放过多报，只查「无」会放过少报。
    #[test]
    fn supported_transports_match_the_assembled_stack() {
        let mut config = EndpointConfig::default();
        config.webrtc_p2p = None;
        let kinds = supported_transports(&config);

        // direct 恒在（浏览器够到原生端的唯一入口，两个 target 都装）
        assert!(kinds.contains(&TransportKind::WebrtcDirect));
        // 打洞跟着配置走
        assert!(!kinds.contains(&TransportKind::Webrtc));

        let has_socket = cfg!(not(wasm_browser));
        assert_eq!(kinds.contains(&TransportKind::Tcp), has_socket);
        assert_eq!(kinds.contains(&TransportKind::Quic), has_socket);

        // native 未启用时不该声称支持 —— 多报会让用户配下一条永远连不上的引导节点，
        // 且没有任何提示。（浏览器侧跟着 API 探测走，不在这条断言的范围内。）
        #[cfg(not(wasm_browser))]
        assert!(!kinds.contains(&TransportKind::Webtransport));
    }

    /// native 侧 WebTransport 跟着「有没有启用」走，**不跟证书端口走** ——
    /// 纯拨号方（`client_only`）同样支持它。
    ///
    /// 「有」和「无」两侧都要查 —— 只查其一会放过多报或少报（见 `supported_transports`
    /// 的文档）。
    #[cfg(not(wasm_browser))]
    #[test]
    fn webtransport_appears_once_enabled_regardless_of_store() {
        let mut config = EndpointConfig::default();
        assert!(!supported_transports(&config).contains(&TransportKind::Webtransport));

        // 纯拨号也算支持：它拨得动 `/quic-v1/webtransport`。
        config.webtransport = Some(crate::config::WebTransportConfig::client_only());
        assert!(supported_transports(&config).contains(&TransportKind::Webtransport));
    }

    #[test]
    fn hole_punching_transport_appears_only_when_configured() {
        let mut config = EndpointConfig::default();
        config.webrtc_p2p = Some(crate::config::WebRtcP2pConfig::default());
        assert!(supported_transports(&config).contains(&TransportKind::Webrtc));
    }
}
