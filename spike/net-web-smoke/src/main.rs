//! native 侧：`helper` / `dial` 两个子命令。
//!
//! ```sh
//! # 终端 1：LanHelper 形态（relay server + ws + webrtc-direct listener + echo 服务）
//! cargo run -- helper
//! # 终端 2：拨浏览器的 circuit 地址跑反向 echo
//! cargo run -- dial "<circuit-addr>"
//! ```

#![cfg(not(target_arch = "wasm32"))]

use anyhow::{Context, Result, bail};
use net_web_smoke::proto::{EchoReq, EchoService, SMOKE_ECHO};
use swarmdrop_net::{
    Endpoint, NodeAddr, RelayServerConfig, Router, presets,
};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,swarmdrop_net=debug".into()),
        )
        .init();

    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("helper") => helper().await,
        Some("dial") => {
            let addr = args.next().context("用法: dial <multiaddr-with-/p2p/id>")?;
            dial(&addr).await
        }
        _ => bail!("用法: net-web-smoke helper | dial <addr>"),
    }
}

/// LanHelper：ws + webrtc-direct 监听（浏览器的两条入口）+ relay server
/// （浏览器 circuit listen 的落点）+ echo 服务。
async fn helper() -> Result<()> {
    let endpoint = Endpoint::builder()
        .preset(presets::Native)
        .identify_protocol("/swarmdrop/2.0.0")
        .agent_version("net-web-smoke/helper")
        .listen(vec![
            "/ip4/0.0.0.0/tcp/0".parse().unwrap(),
            "/ip4/0.0.0.0/tcp/0/ws".parse().unwrap(),
            "/ip4/0.0.0.0/udp/0/webrtc-direct".parse().unwrap(),
        ])
        .relay_server(RelayServerConfig::default())
        .bind()
        .await?;

    let _router = Router::builder(endpoint.clone())
        .accept(SMOKE_ECHO.protocol(), SMOKE_ECHO.handler(EchoService))
        .spawn();

    println!("helper node id: {}", endpoint.node_id());

    // 持续打印监听地址（含 ws 与 webrtc-direct certhash，浏览器从中选拨）
    let mut watcher = endpoint.watch_addrs();
    let id = endpoint.node_id();
    tokio::spawn(async move {
        loop {
            let info = watcher.get();
            println!("── listen addrs ──");
            for a in &info.listen {
                println!("  {a}/p2p/{id}");
            }
            if watcher.updated().await.is_none() {
                break;
            }
        }
    });

    tokio::signal::ctrl_c().await?;
    endpoint.close().await;
    Ok(())
}

/// 第二 native 节点：拨浏览器的 circuit 地址，发起 echo（验证浏览器被动接收）。
async fn dial(addr: &str) -> Result<()> {
    let (addr_part, id) = addr
        .trim()
        .rsplit_once("/p2p/")
        .context("地址须以 /p2p/<node-id> 结尾")?;
    let node = id.parse()?;
    let addr = addr_part.parse()?;

    let endpoint = Endpoint::builder()
        .preset(presets::Native)
        .identify_protocol("/swarmdrop/2.0.0")
        .agent_version("net-web-smoke/dialer")
        .bind()
        .await?;

    let info = endpoint
        .connect(NodeAddr::with_addrs(node, vec![addr]))
        .await?;
    println!("connected: path={:?} addr={}", info.path, info.addr);

    let resp = SMOKE_ECHO
        .call(
            &endpoint,
            node,
            &EchoReq {
                text: "hello from native dialer".into(),
            },
        )
        .await?;
    println!("echo ok: {:?} (remote saw us as {})", resp.text, resp.from);

    endpoint.close().await;
    Ok(())
}
