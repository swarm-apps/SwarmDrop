//! 一个最小的 `/webrtc-direct` 监听端，用来手动验证浏览器侧的拨号。
//!
//! 自动化测试只能覆盖 native↔native（`tests/direct_loopback.rs`）；浏览器那半必须真的
//! 用浏览器跑一遍——wasm 代码编过不代表逻辑对，这个教训在本项目已经吃过几次。
//!
//! ```console
//! $ cargo run -p webrtc-p2p --example direct_listener
//! 监听地址（粘到浏览器里拨）：
//!   /ip4/192.168.1.5/udp/54321/webrtc-direct/certhash/uEi…/p2p/12D3Koo…
//! ```
//!
//! 每次运行都会新生成一张证书，所以 certhash 每次都变——手动测试无所谓，
//! 生产端必须持久化（见 `DirectConfig::with_certificate_pem`）。

use std::pin::Pin;

use futures::future::poll_fn;
use libp2p_core::Transport as _;
use libp2p_core::transport::{ListenerId, TransportEvent};
use libp2p_identity::Keypair;
use webrtc_p2p::{Config, DirectConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "webrtc_p2p=debug".into()),
        )
        .init();

    // `libp2p-identity` 的随机生成要 `rand` feature，本 crate 没开——手动测试用固定
    // 种子即可（每次跑得到同一个 PeerId，反倒省得改浏览器里的地址）。
    let keypair = Keypair::ed25519_from_bytes([7u8; 32])?;
    let peer_id = keypair.public().to_peer_id();

    let config = Config::default().with_direct(DirectConfig::new(keypair));
    let (mut transport, _behaviour) = webrtc_p2p::new(
        config,
        std::sync::Arc::new(|_: &Config| {
            Err(webrtc_p2p::BackendError::new("本示例只走 direct 路径"))
        }),
    );

    transport.listen_on(
        ListenerId::next(),
        "/ip4/0.0.0.0/udp/0/webrtc-direct".parse()?,
    )?;

    println!("\n本机节点：{peer_id}\n监听地址（粘到浏览器里拨）：");

    loop {
        match poll_fn(|cx| Pin::new(&mut transport).poll(cx)).await {
            TransportEvent::NewAddress { listen_addr, .. } => {
                // 补上 `/p2p` 段——对端要靠它认出目标，swarm 在这一层不代劳。
                println!("  {listen_addr}/p2p/{peer_id}");
            }
            TransportEvent::Incoming { upgrade, .. } => {
                tokio::spawn(async move {
                    match upgrade.await {
                        Ok((peer, _conn)) => println!("\n✅ 入站连接就绪，对端 = {peer}\n"),
                        Err(e) => println!("\n❌ 入站连接失败：{e}\n"),
                    }
                });
            }
            TransportEvent::ListenerClosed { reason, .. } => {
                println!("监听器关闭：{reason:?}");
                return Ok(());
            }
            _ => {}
        }
    }
}
