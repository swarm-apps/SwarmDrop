//! 浏览器侧：从当前页面拨一个 multiaddr，报告成败。
//!
//! # 这个 spike 到底在问什么
//!
//! `dev-notes/knowledge/libp2p-wasm.md` 里有一张表，断言：
//!
//! | relay 在哪 | `ws://` | `webrtc-direct` |
//! |---|---|---|
//! | 私有 IP | ✅ 私有 IP 字面量豁免 mixed content | ✅ certhash |
//! | 公网裸 IP | ❌ mixed content 拦死 | ✅ certhash |
//!
//! 右列的地基是「**WebRTC 不受 mixed content 约束**」—— 而那条**只有间接证据，没有规范原文**。
//! 整个「用户自建 relay 免域名」的架构论据都压在它上面。本 spike 就是去证伪它。
//!
//! # 为什么官方例子答不了这个问题
//!
//! `rust-libp2p/examples/browser-webrtc` 已经证明「浏览器 → 局域网 IP 的 webrtc-direct」可行
//! （它的 `main.rs:49-59` 监听 `0.0.0.0` 后主动跳过 localhost，浏览器拨的本来就是 LAN IP）。
//! **但它用 axum 走 `http://` 提供页面 —— HTTP 页面根本没有 mixed content 约束，测不到那道门。**
//!
//! 所以本 spike 只改一个变量：**把页面换成 HTTPS**，其余照抄官方。

#![cfg(target_arch = "wasm32")]

use futures::{FutureExt, StreamExt};
// websys transport 走 facade 的 re-export（libp2p-0.56.0/src/lib.rs:128,135），
// 不用直接依赖 libp2p-webrtc-websys —— 那样版本容易和 facade 里的对不上。
use libp2p::{
    Multiaddr, core::muxing::StreamMuxerBox, core::transport::Transport as _,
    core::upgrade::Version, noise, ping, swarm::SwarmEvent, webrtc_websys, websocket_websys,
    yamux,
};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
fn start() {
    tracing_subscriber::fmt()
        // 浏览器里不 without_time() 会 runtime error（wasm 无 std 时钟）
        .without_time()
        .with_ansi(false)
        // 默认是 INFO —— libp2p_webrtc_websys 的握手细节全在 DEBUG，不开就是瞎子。
        .with_max_level(tracing::Level::DEBUG)
        .with_writer(tracing_subscriber_wasm::MakeConsoleWriter::default())
        .init();
}

/// 当前页面的 origin —— 报告里必须带上，否则拿到结果也不知道测的是哪一格。
#[wasm_bindgen]
pub fn page_origin() -> String {
    web_sys::window()
        .and_then(|w| w.location().origin().ok())
        .unwrap_or_else(|| "<unknown>".to_string())
}

/// 隔离用：**只装 webrtc**，其余与 `dial` 完全相同。
///
/// 排查「我的 or_transport 组合有问题」vs「0.9.0-alpha.1/0.4.0 这对版本本身有问题」——
/// 官方例子（master 的 0.10.0-alpha/0.5.0，且只装 webrtc）实测能通，我这边超时，
/// 两个变量必须分开验。
#[wasm_bindgen]
pub async fn dial_webrtc_only(addr: String) -> Result<String, JsError> {
    dial_inner(addr, true).await
}

/// 拨一个 multiaddr 并 ping，成功返回 RTT 描述。
///
/// webrtc-direct 与 ws 两个 transport 都装上，靠 multiaddr 自己分派 ——
/// 这样同一个按钮能测 2×2 矩阵的四格。
#[wasm_bindgen]
pub async fn dial(addr: String) -> Result<String, JsError> {
    dial_inner(addr, false).await
}

async fn dial_inner(addr: String, webrtc_only: bool) -> Result<String, JsError> {
    let addr: Multiaddr = addr
        .trim()
        .parse()
        .map_err(|e| JsError::new(&format!("multiaddr 解析失败: {e}")))?;

    let mut swarm = libp2p::SwarmBuilder::with_new_identity()
        .with_wasm_bindgen()
        .with_other_transport(|key| {
            // webrtc-websys 自带 noise + 分帧，不需要 upgrade 链。
            let webrtc = webrtc_websys::Transport::new(webrtc_websys::Config::new(key))
                .map(|(p, c), _| (p, StreamMuxerBox::new(c)));

            if webrtc_only {
                return Ok(webrtc.boxed());
            }

            // websocket-websys 没有便捷方法，必须手动 upgrade/authenticate/multiplex。
            // 写法照 rust-libp2p/interop-tests/src/arch.rs:245-260（官方真跑 CI 的组合）。
            let ws = websocket_websys::Transport::default()
                .upgrade(Version::V1Lazy)
                .authenticate(noise::Config::new(key)?)
                .multiplex(yamux::Config::default())
                .map(|(p, c), _| (p, StreamMuxerBox::new(c)));

            // 两道坎，都会报 E0271：
            // 1. `or_transport` 要求两侧 Output 类型完全一致 —— 一个给
            //    `webrtc_websys::Connection`、一个给 `yamux::Muxer<..>`。故先各自
            //    map 成 StreamMuxerBox（官方原生例子同解）。
            // 2. 摊平后 `OrTransport::Output` 仍是 `future::Either<A, B>`
            //    （libp2p-core-0.43.2/src/transport/choice.rs:51），即便两侧同类型也不会
            //    自动塌缩。SwarmBuilder 要的是 `(PeerId, _)`，得再 `into_inner()` 一次。
            Ok(webrtc.or_transport(ws).map(|either, _| either.into_inner()).boxed())
        })?
        .with_behaviour(|_| ping::Behaviour::new(ping::Config::new()))?
        .with_swarm_config(|c| {
            c.with_idle_connection_timeout(std::time::Duration::from_secs(30))
        })
        .build();

    tracing::info!(%addr, "dialing");
    swarm.dial(addr.clone())?;

    // 拨号 + 首次 ping 的结果。20s 兜底：mixed content 被拦时浏览器**不一定报错**，
    // 可能只是静默不发包 —— 那种情况下等超时才是唯一的观测手段。
    // `select!` 要 FusedFuture —— 裸 async fn 不满足，得 .fuse()。
    let deadline = wasm_timeout(std::time::Duration::from_secs(20)).fuse();
    futures::pin_mut!(deadline);

    loop {
        futures::select! {
            _ = deadline => {
                return Err(JsError::new(
                    "20s 无结论：既没连上也没报错。\
                     这是 mixed content 静默拦截的典型表现 —— 查 DevTools Console/Network。",
                ));
            }
            ev = swarm.select_next_some() => match ev {
                SwarmEvent::Behaviour(ping::Event { result: Ok(rtt), peer, .. }) => {
                    return Ok(format!("✅ 连通并 ping 成功：RTT {rtt:?}，对端 {peer}"));
                }
                SwarmEvent::Behaviour(ping::Event { result: Err(e), .. }) => {
                    return Err(JsError::new(&format!("已连上但 ping 失败: {e}")));
                }
                SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                    tracing::info!(%peer_id, "connection established, waiting for ping");
                }
                SwarmEvent::OutgoingConnectionError { error, .. } => {
                    // 拨号失败的具体原因是本 spike 最有价值的输出：
                    // 是被浏览器拦了，还是 SDP/certhash/noise 出错？
                    return Err(JsError::new(&format!("❌ 拨号失败: {error}")));
                }
                other => tracing::debug!(?other, "swarm event"),
            }
        }
    }
}

/// wasm 上没有 tokio::time —— 用 futures-timer（libp2p 已把它带进 wasm 树，零新依赖）。
async fn wasm_timeout(d: std::time::Duration) {
    futures_timer::Delay::new(d).await
}
