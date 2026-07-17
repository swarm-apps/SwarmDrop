//! 服务端：一个 libp2p 节点（同时监听 webrtc-direct 和 ws）+ 两个静态站点（http 和 https）。
//!
//! 设计要点：**同一份页面同时挂在 http:// 和 https:// 两个 origin 上**。
//! 这样 2×2 对照只需换 URL，其余变量全同 —— 否则「失败」分不清是 mixed content
//! 还是别的差异。
//!
//! ```text
//!            ┌─ http://<LAN-IP>:8080   ← 基线（等价官方例子，已知可行）
//! 同一份 wasm ┤
//!            └─ https://<LAN-IP>:8443  ← 待验（mixed content 是否拦）
//!                     │
//!                     ├─ dial /ip4/<LAN-IP>/udp/<p>/webrtc-direct/certhash/…  ← 最重要
//!                     └─ dial /ip4/<LAN-IP>/tcp/<p>/ws
//! ```

#![cfg(not(target_arch = "wasm32"))]

use std::{net::SocketAddr, path::PathBuf};

use anyhow::{Context, Result};
use axum::Router;
use futures::StreamExt;
use libp2p::{
    Multiaddr, Swarm,
    core::{muxing::StreamMuxerBox, transport::Transport as _},
    multiaddr::Protocol,
    ping,
    swarm::SwarmEvent,
};
use libp2p_webrtc as webrtc;
use tower_http::{cors::CorsLayer, services::ServeDir};

const HTTP_PORT: u16 = 8080;
const HTTPS_PORT: u16 = 8443;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| {
                "webrtc_direct_https_spike=debug,libp2p_webrtc=info,libp2p_ping=debug".into()
            }),
        )
        .init();

    // 依赖树里 ring 和 aws-lc-rs 同时存在（libp2p-webrtc 与 axum-server 各拉一个），
    // rustls 无法自动选，panic 在 axum-server 的 unwrap 里。必须显式装进程默认。
    //
    // ⚠️ 对照 spike/iroh-web 的坑 3：那里 `install_default()` **无效**，因为 iroh 读的是
    // builder 字段而非进程默认。同一个报错、相反的解 —— 别把两边的经验互相套用。
    rustls::crypto::ring::default_provider()
        .install_default()
        .map_err(|_| anyhow::anyhow!("安装 rustls crypto provider 失败"))?;

    let lan_ip = detect_lan_ip().context(
        "找不到非 loopback 的 IPv4 地址。本 spike 必须用 LAN IP —— \
         localhost 在 mixed content 和 LNA 里都是特例，测了不算数。",
    )?;

    let mut swarm = build_swarm().await?;

    // 监听两个 transport。webrtc-direct 用 UDP，ws 用 TCP。
    swarm.listen_on(
        Multiaddr::empty()
            .with(Protocol::Ip4(std::net::Ipv4Addr::UNSPECIFIED))
            .with(Protocol::Udp(0))
            .with(Protocol::WebRTCDirect),
    )?;
    // webrtc-only 模式下没有 ws transport，listen 会直接报 MultiaddrNotSupported。
    if std::env::var("SPIKE_WEBRTC_ONLY").is_err() {
        swarm.listen_on(
            Multiaddr::empty()
                .with(Protocol::Ip4(std::net::Ipv4Addr::UNSPECIFIED))
                .with(Protocol::Tcp(0))
                .with(Protocol::Ws("/".into())),
        )?;
    }

    // 收集两条 LAN 地址后再起站点 —— 页面要把它们印出来给人复制。
    let (webrtc_addr, ws_addr) = collect_listen_addrs(&mut swarm, lan_ip).await?;

    let static_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static");
    write_addrs_js(&static_dir, &webrtc_addr, &ws_addr)?;

    let app = Router::new()
        .fallback_service(ServeDir::new(&static_dir))
        .layer(CorsLayer::permissive());

    // 自签证书：SAN 覆盖 LAN IP。浏览器会告警，点「继续」即可 ——
    // **点过之后 origin 仍然是 https://，mixed content 规则照常生效**，不影响本测。
    let tls = self_signed_tls(lan_ip).await?;

    println!("\n{}", "=".repeat(78));
    println!("  webrtc-direct × HTTPS spike");
    println!("{}", "=".repeat(78));
    println!("\n  两个 origin，同一份页面 —— 唯一的变量就是 http vs https：\n");
    println!("    基线（应该通）  http://{lan_ip}:{HTTP_PORT}");
    println!("    待验（问题所在）https://{lan_ip}:{HTTPS_PORT}   ← 会有自签证书告警，点继续");
    println!("\n  页面里已填好这两个地址：");
    println!("    webrtc-direct  {webrtc_addr}");
    println!("    ws             {ws_addr}");
    println!("\n{}\n", "=".repeat(78));

    let http = tokio::spawn(serve_http(app.clone(), SocketAddr::from(([0, 0, 0, 0], HTTP_PORT))));
    let https = tokio::spawn(serve_https(app, SocketAddr::from(([0, 0, 0, 0], HTTPS_PORT)), tls));

    // swarm 必须持续被 poll，否则连接建不起来。
    let net = tokio::spawn(async move {
        loop {
            let ev = swarm.select_next_some().await;
            match ev {
                SwarmEvent::Behaviour(ping::Event { peer, result, .. }) => {
                    tracing::info!(%peer, ?result, "ping");
                }
                SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                    tracing::info!(%peer_id, ?endpoint, "✅ 浏览器连上了");
                }
                other => tracing::debug!(?other, "swarm"),
            }
        }
    });

    tokio::select! {
        r = http  => r??,
        r = https => r??,
        r = net   => r?,
        _ = tokio::signal::ctrl_c() => println!("\n收工"),
    }
    Ok(())
}

/// SwarmBuilder 的 phase 链是**有序的**，`with_websocket` 只存在于 `WebsocketPhase`：
///
/// ```text
/// Provider ─▶ Tcp ─▶ Quic ─▶ OtherTransport ─▶ Dns ─▶ Websocket ─▶ Behaviour
/// ```
///
/// 所以 `with_other_transport(...)` 之后必须先 `.with_dns()` 才能拿到 `.with_websocket()`
/// （`libp2p-0.56.0/src/builder/phase/other_transport.rs:82`）。直接串会报
/// 「no method named `with_websocket` found for SwarmBuilder<Tokio, OtherTransportPhase<..>>」。
/// 且 `with_websocket` 是 **async**。
async fn build_swarm() -> Result<Swarm<ping::Behaviour>> {
    // `SPIKE_WEBRTC_ONLY=1` 把服务端降到与官方例子完全相同的配置（只有 webrtc，无 dns/ws）。
    // 用于隔离「我加的 dns/ws 破坏了 webrtc」vs「已发布版本本身不通」。
    if std::env::var("SPIKE_WEBRTC_ONLY").is_ok() {
        tracing::warn!("SPIKE_WEBRTC_ONLY：服务端只装 webrtc（对齐官方例子），ws 按钮将失效");
        return Ok(libp2p::SwarmBuilder::with_new_identity()
            .with_tokio()
            .with_other_transport(|id_keys| {
                Ok(webrtc::tokio::Transport::new(
                    id_keys.clone(),
                    webrtc::tokio::Certificate::generate(&mut rand::thread_rng())?,
                )
                .map(|(peer, conn), _| (peer, StreamMuxerBox::new(conn))))
            })?
            .with_behaviour(|_| ping::Behaviour::new(ping::Config::new()))?
            .build());
    }

    Ok(libp2p::SwarmBuilder::with_new_identity()
        .with_tokio()
        // webrtc-direct：证书是**自签**的，指纹进 multiaddr 的 /certhash ——
        // 这正是「免 CA、裸 IP 可用」的机制所在，也是本 spike 要验的东西。
        .with_other_transport(|id_keys| {
            Ok(webrtc::tokio::Transport::new(
                id_keys.clone(),
                webrtc::tokio::Certificate::generate(&mut rand::thread_rng())?,
            )
            .map(|(peer, conn), _| (peer, StreamMuxerBox::new(conn))))
        })?
        .with_dns()?
        // ws：走标准 upgrade 链，用来对照「私有 IP 是否豁免 mixed content」。
        .with_websocket(libp2p::noise::Config::new, libp2p::yamux::Config::default)
        .await?
        .with_behaviour(|_| ping::Behaviour::new(ping::Config::new()))?
        .build())
}

/// 等到 webrtc-direct 和 ws 各自报出一条 **LAN IP** 地址。
///
/// 必须挑 LAN IP，不能用 localhost：mixed content 与 LNA 对 loopback 都有特例，
/// 用 localhost 测出来的「通过」是假绿灯。官方例子同样跳过 localhost
/// （examples/browser-webrtc/src/main.rs:49-59）。
async fn collect_listen_addrs(
    swarm: &mut Swarm<ping::Behaviour>,
    lan_ip: std::net::Ipv4Addr,
) -> Result<(Multiaddr, Multiaddr)> {
    let (mut webrtc_addr, mut ws_addr) = (None, None);

    let need_ws = std::env::var("SPIKE_WEBRTC_ONLY").is_err();
    while webrtc_addr.is_none() || (need_ws && ws_addr.is_none()) {
        if let SwarmEvent::NewListenAddr { address, .. } = swarm.select_next_some().await {
            if !address.iter().any(|p| p == Protocol::Ip4(lan_ip)) {
                continue; // 跳过 loopback / 其他网卡
            }
            let peer_id = *swarm.local_peer_id();
            let full = address.clone().with(Protocol::P2p(peer_id));
            if address.iter().any(|p| p == Protocol::WebRTCDirect) {
                tracing::info!(%full, "webrtc-direct listen addr");
                webrtc_addr = Some(full);
            } else if address.iter().any(|p| matches!(p, Protocol::Ws(_))) {
                tracing::info!(%full, "ws listen addr");
                ws_addr = Some(full);
            }
        }
    }
    // webrtc-only 模式下没有 ws 地址，给个占位（页面的 ws 按钮此时不该点）。
    Ok((
        webrtc_addr.unwrap(),
        ws_addr.unwrap_or_else(|| "/ip4/0.0.0.0/tcp/0/ws".parse().unwrap()),
    ))
}

/// 把地址写成 js，页面直接 import —— 省掉手抄 certhash（那玩意儿 50 多个字符）。
///
/// 注意用 `"{}"` 而非 `{:?}`：**Multiaddr 的 Debug 不带引号**（打印的就是裸地址串），
/// `{:?}` 生成的是 `export const X = /ip4/...;` —— 无效 JS，页面 import 直接语法错误。
fn write_addrs_js(dir: &PathBuf, webrtc: &Multiaddr, ws: &Multiaddr) -> Result<()> {
    std::fs::write(
        dir.join("addrs.js"),
        format!("export const WEBRTC_ADDR = \"{webrtc}\";\nexport const WS_ADDR = \"{ws}\";\n"),
    )?;
    Ok(())
}

/// 选一块真实局域网网卡。
///
/// **不能瞎选**：开发机上通常还挂着 Tailscale（`utun*`）、Docker/虚拟机网桥（`bridge*`、
/// `vmenet*`）。挑错网卡的话浏览器根本连不上，而我们会误判成「mixed content 拦了」——
/// **假阴性比没结果更糟**。`if_addrs` 的返回顺序不保证，不能靠 `.find()` 撞运气。
///
/// 策略：排除已知的虚拟网卡前缀，把剩余候选全印出来，允许 `SPIKE_LAN_IP` 覆盖。
fn detect_lan_ip() -> Option<std::net::Ipv4Addr> {
    if let Ok(forced) = std::env::var("SPIKE_LAN_IP") {
        return forced.parse().ok();
    }

    const VIRTUAL: &[&str] = &["utun", "bridge", "vmenet", "docker", "veth", "tun", "tap", "awdl"];

    let candidates: Vec<_> = if_addrs::get_if_addrs()
        .ok()?
        .into_iter()
        .filter_map(|i| match i.addr.ip() {
            std::net::IpAddr::V4(v4)
                if !v4.is_loopback()
                    && !v4.is_link_local()
                    && !VIRTUAL.iter().any(|p| i.name.starts_with(p)) =>
            {
                Some((i.name, v4))
            }
            _ => None,
        })
        .collect();

    if candidates.len() > 1 {
        eprintln!("⚠️ 有多块候选网卡，选了第一块。要换请设 SPIKE_LAN_IP：");
        for (n, ip) in &candidates {
            eprintln!("     {n:10} {ip}");
        }
    }
    candidates.into_iter().next().map(|(_, ip)| ip)
}

async fn self_signed_tls(ip: std::net::Ipv4Addr) -> Result<axum_server::tls_rustls::RustlsConfig> {
    let cert = rcgen::generate_simple_self_signed(vec![ip.to_string(), "localhost".into()])?;
    axum_server::tls_rustls::RustlsConfig::from_pem(
        cert.cert.pem().into_bytes(),
        cert.key_pair.serialize_pem().into_bytes(),
    )
    .await
    .context("加载自签证书失败")
}

async fn serve_http(app: Router, addr: SocketAddr) -> Result<()> {
    let l = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(l, app).await?;
    Ok(())
}

async fn serve_https(
    app: Router,
    addr: SocketAddr,
    tls: axum_server::tls_rustls::RustlsConfig,
) -> Result<()> {
    axum_server::bind_rustls(addr, tls)
        .serve(app.into_make_service())
        .await?;
    Ok(())
}
