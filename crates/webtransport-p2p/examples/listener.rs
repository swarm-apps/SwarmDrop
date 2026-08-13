//! 起一个 WebTransport listener，打印可拨地址 —— 浏览器手测的入口。
//!
//! ```console
//! $ cargo run -p webtransport-p2p --example listener
//! $ cargo run -p webtransport-p2p --example listener -- 4004   # 指定端口
//! ```
//!
//! # 浏览器侧怎么验
//!
//! 打印出来的地址形如
//! `/ip4/192.168.1.5/udp/4004/quic-v1/webtransport/certhash/<h1>/certhash/<h2>`。
//! 在浏览器控制台里：
//!
//! ```js
//! // certhash 是 multibase base64url（`u` 开头），要先去掉前缀再 base64url 解码成 32 字节
//! const wt = new WebTransport(
//!   "https://192.168.1.5:4004/.well-known/libp2p-webtransport?type=noise",
//!   { serverCertificateHashes: [{ algorithm: "sha-256", value: bytes }] },
//! );
//! await wt.ready;   // 到这一步说明 TLS + CONNECT 都过了
//! ```
//!
//! `wt.ready` 成功即证明三件事：自签名证书被浏览器接受（有效期 ≤14 天 + ECDSA P-256）、
//! `:path` 与 `?type=noise` 判据放行、QUIC 链路通。**Noise 握手不在这条路径上** ——
//! 那需要完整的 libp2p 客户端（js-libp2p 或本仓的 wasm 端）。
//!
//! 证书**不持久化**：每次运行 certhash 都会变，这正是手测想要的（免得旧地址干扰）。
//! 生产路径见 `Config::with_certificate_store`。

use std::error::Error;
use std::pin::Pin;

use futures::future::poll_fn;
use libp2p_core::Transport as _;
use libp2p_core::transport::{ListenerId, TransportEvent};
use libp2p_identity::Keypair;
use webtransport_p2p::{Config, Transport};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "webtransport_p2p=debug,wtransport=info".into()),
        )
        .init();

    let port: u16 = match std::env::args().nth(1) {
        Some(arg) => arg
            .parse()
            .map_err(|_| format!("无法解析端口 {arg:?}（例：-- 4004）"))?,
        None => 4004,
    };

    let keypair = Keypair::generate_ed25519();
    let peer_id = keypair.public().to_peer_id();
    let mut transport = Transport::new(Config::new(keypair))?;

    transport.listen_on(
        ListenerId::next(),
        format!("/ip4/0.0.0.0/udp/{port}/quic-v1/webtransport").parse()?,
    )?;

    println!("PeerId: {peer_id}");
    println!("按 Ctrl+C 退出\n");

    loop {
        match poll_fn(|cx| Pin::new(&mut transport).poll(cx)).await {
            TransportEvent::NewAddress { listen_addr, .. } => {
                // 带上 `/p2p/<id>`：完整可拨地址，可直接贴进另一端的引导节点配置。
                println!("可拨地址: {listen_addr}/p2p/{peer_id}");
            }
            TransportEvent::Incoming {
                upgrade,
                send_back_addr,
                ..
            } => {
                println!("入站连接 ← {send_back_addr}");
                tokio::spawn(async move {
                    match upgrade.await {
                        Ok((peer, _muxer)) => println!("  ✓ Noise 认证完成，对端 {peer}"),
                        Err(e) => println!("  ✗ 升级失败：{e}"),
                    }
                });
            }
            TransportEvent::AddressExpired { listen_addr, .. } => {
                println!("地址失效（证书已轮换）: {listen_addr}");
            }
            TransportEvent::ListenerClosed { reason, .. } => {
                println!("监听器关闭：{reason:?}");
                return Ok(());
            }
            TransportEvent::ListenerError { error, .. } => println!("监听错误：{error}"),
        }
    }
}
