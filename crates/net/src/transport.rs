//! Swarm 组装：transport 栈按 target 静态构建。
//!
//! | target | transports |
//! |---|---|
//! | native | TCP + QUIC + DNS + WebSocket(listen 给浏览器) + relay client |
//! | wasm   | webrtc-websys + websocket-websys + relay client（不能 listen 本地 socket）|
//!
//! 说明：
//! - SwarmBuilder 是编译期类型状态链，无法按运行时配置增删 transport；
//!   「启不启用」用 behaviour 侧 `Toggle`（relay）或「不 listen」（tcp/ws）表达，
//!   builder 链本身保持单条不分叉（旧栈同款手法）。
//! - relay phase 无 target gate（`cfg(feature = "relay")` only）——wasm 也走
//!   `with_relay_client`，浏览器经 circuit 被动接收连接靠它。
//! - webrtc-direct（native 侧 server/dialer）在 M2 经 `with_other_transport` 接入。

use libp2p::Swarm;
use libp2p::identity::Keypair;

use crate::behaviour::Behaviour;
use crate::config::EndpointConfig;

/// Swarm 组装失败。
#[derive(Debug, thiserror::Error)]
#[error("failed to build swarm: {0}")]
pub struct BuildSwarmError(String);

#[cfg(not(wasm_browser))]
pub(crate) async fn build_swarm(
    keypair: Keypair,
    config: &EndpointConfig,
) -> Result<Swarm<Behaviour>, BuildSwarmError> {
    use libp2p::{SwarmBuilder, noise, tcp, yamux};

    let err = |e: &dyn std::fmt::Display| BuildSwarmError(e.to_string());

    let swarm = SwarmBuilder::with_existing_identity(keypair)
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
            use libp2p::core::muxing::StreamMuxerBox;
            use libp2p::core::transport::Transport as _;
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
                .map(|(peer, conn), _| (peer, StreamMuxerBox::new(conn))))
        })
        .map_err(|e| err(&e))?
        // Android 上 system resolver（/etc/resolv.conf）不存在会失败——M4 宿主接线时
        // 用 with_dns_config 显式配置处理，桌面/服务器场景 system 即可。
        .with_dns()
        .map_err(|e| err(&e))?
        // WebSocket listener：LanHelper 给浏览器的入口（ws:// 私有 IP 豁免
        // mixed content，spike 实证）。是否真的 listen 由 listen 地址决定。
        .with_websocket(noise::Config::new, yamux::Config::default)
        .await
        .map_err(|e| err(&e))?
        .with_relay_client(noise::Config::new, yamux::Config::default)
        .map_err(|e| err(&e))?
        .with_behaviour(|key, relay_client| {
            Behaviour::new(key, config.relay_client.then_some(relay_client), config)
        })
        .map_err(|e| err(&e))?
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(config.idle_timeout))
        .build();

    Ok(swarm)
}

#[cfg(wasm_browser)]
pub(crate) async fn build_swarm(
    keypair: Keypair,
    config: &EndpointConfig,
) -> Result<Swarm<Behaviour>, BuildSwarmError> {
    use libp2p::core::muxing::StreamMuxerBox;
    use libp2p::core::transport::Transport as _;
    use libp2p::core::upgrade::Version;
    use libp2p::{SwarmBuilder, noise, webrtc_websys, websocket_websys, yamux};

    let err = |e: &dyn std::fmt::Display| BuildSwarmError(e.to_string());

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
            let webrtc = webrtc_websys::Transport::new(webrtc_websys::Config::new(key))
                .map(|(p, c), _| (p, StreamMuxerBox::new(c)));

            // or_transport 两道坎（E0271）：两侧先各自 map 成 StreamMuxerBox；
            // 摊平后 Output 仍是 future::Either，需 into_inner() 再塌缩一次。
            Ok(webrtc
                .or_transport(ws)
                .map(|either, _| either.into_inner())
                .boxed())
        })
        .map_err(|e| err(&e))?
        .with_relay_client(noise::Config::new, yamux::Config::default)
        .map_err(|e| err(&e))?
        .with_behaviour(|key, relay_client| {
            Behaviour::new(key, config.relay_client.then_some(relay_client), config)
        })
        .map_err(|e| err(&e))?
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(config.idle_timeout))
        .build();

    Ok(swarm)
}
