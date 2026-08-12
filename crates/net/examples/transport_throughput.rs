//! 同机回环下逐 transport 对比数据面吞吐：**QUIC vs TCP vs WebRTC-direct vs WebTransport**。
//!
//! ```console
//! $ cargo run -p swarmdrop-net --release --example transport_throughput
//! $ cargo run -p swarmdrop-net --release --example transport_throughput -- 256   # 每档 256 MiB
//! ```
//!
//! # 为什么是这个形状
//!
//! 真机上 QUIC 有 12–23 MB/s 而 WebRTC 只有 0.36–0.96 MB/s，但那是**两条不同的网络
//! 路径**（局域网直连 vs 打洞），无法归因。这里让三者跑在**同一台机器、同一个回环、
//! 同一个应用层**（`Endpoint` + `P2pStream`）上，只换 transport：网络变量被消掉，
//! 剩下的差距就只能来自各自的数据面实现。
//!
//! # 与 `webrtc-p2p/examples/throughput.rs` 的区别（这一条是关键）
//!
//! 那个基准直接拿 `libp2p_core::Transport` 手写 poll 循环，于是**测量装置自己成了主要
//! 误差源**：它一度把「4 MiB 全塞进发送缓冲」读成 104 MB/s，又因为把 transport 驱动和
//! 传输绑在同一个 task 上而自锁挂死（详见
//! `dev-notes/research/2026-08-11-web-webrtc-throughput.md` §4）。
//!
//! 这里改用 `Endpoint`——它**自带后台 actor 事件循环**，调用方不需要、也没办法手动
//! 驱动 transport。既是生产路径，也消掉了那一整类误差。

use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::{AsyncReadExt, AsyncWriteExt};
use swarmdrop_net::{
    AcceptError, Addr, Endpoint, NodeAddr, P2pStream, ProtocolHandler, ProtocolId, Router,
    WebTransportConfig, WebTransportMemoryCertificateStore,
};

const SINK: ProtocolId = ProtocolId::from_static("/bench/sink/1");

/// 每档超时。WebRTC 若真是 ~1 MB/s，64 MiB 要一分多钟，故给得宽。
const PER_CASE_TIMEOUT: Duration = Duration::from_secs(600);

/// 读到 EOF 后回一个字节数——让发送侧能测「端到端真正收全」的时间。
///
/// 不用 echo：回传等量数据会把测量变成往返带宽，且接收侧的写会与发送侧的读互相干扰。
#[derive(Debug, Clone)]
struct Sink;

impl ProtocolHandler for Sink {
    async fn accept(&self, mut stream: P2pStream) -> Result<(), AcceptError> {
        let mut buf = vec![0u8; 256 * 1024];
        let mut total: u64 = 0;
        loop {
            let n = stream.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            total += n as u64;
        }
        stream.write_all(&total.to_le_bytes()).await?;
        stream.close().await?;
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .init();

    // 解析失败必须报错，**不能静默回退**：`-- 256M` 或 `--release 256` 这类手误会让它
    // 按 64 MiB 跑，而表头照样印着「64 MiB」——数字随后被引用进文档，错得毫无痕迹。
    let total_mib: usize = match std::env::args().nth(1) {
        Some(arg) => arg
            .parse()
            .map_err(|_| format!("无法解析大小参数 {arg:?}（单位 MiB，例：-- 256）"))?,
        None => 64,
    };
    let total = total_mib * 1024 * 1024;

    // 每档只监听**一个** transport：服务端若同时听多个，拨号方可能选中另一条，
    // 于是测出来的根本不是标称的那个（这类误配很难从结果上看出来）。
    let cases: &[(&str, &str, &str)] = &[
        ("/ip4/127.0.0.1/udp/0/quic-v1", "quic-v1", "QUIC"),
        ("/ip4/127.0.0.1/tcp/0", "/tcp/", "TCP+Noise+yamux"),
        (
            "/ip4/127.0.0.1/udp/0/webrtc-direct",
            "webrtc-direct",
            "WebRTC-direct",
        ),
        (
            "/ip4/127.0.0.1/udp/0/quic-v1/webtransport",
            "webtransport",
            "WebTransport",
        ),
    ];

    eprintln!("每档传 {total_mib} MiB（同机回环 · 同一 Endpoint 应用层 · 只换 transport）\n");
    eprintln!(
        "⚠️  回环基准方差极大（WebRTC-direct 实测 51–203 MiB/s）。\
         **单次数字不可比**，至少取 6 次中位数再引用。\n"
    );
    eprintln!("{:<18}  {:>10}  {:>13}", "transport", "耗时", "吞吐");

    // 超时**在 `bench` 内部**施加，不在这里包 —— 从外面 `timeout` 会把 future 连同它
    // 未跑到的清理代码一起 drop，把两个活着的 Endpoint 留给下一档。
    for (listen, needle, label) in cases {
        match bench(listen, needle, total).await {
            Ok(elapsed) => {
                // MiB/s（1024²），不是 MB/s —— 两者差 4.9%，而这些数字会被引用进文档
                // 并与真机报告里的 MB/s 交叉比较。
                let mib_s = total as f64 / elapsed.as_secs_f64() / (1024.0 * 1024.0);
                eprintln!(
                    "{:<18}  {:>9.2}s  {:>9.2} MiB/s",
                    label,
                    elapsed.as_secs_f64(),
                    mib_s
                );
            }
            Err(e) => eprintln!("{label:<18}  失败：{e}"),
        }
    }
    Ok(())
}

/// 建一对 `Endpoint`、传 `total` 字节，返回「写完并被对端全部收到」的耗时。
async fn bench(listen: &str, needle: &str, total: usize) -> Result<Duration, Box<dyn Error>> {
    // WebTransport 档两端都要装配 transport（不装就拨不出去，地址会以
    // MultiaddrNotSupported 快速失败），但**只有监听方需要证书存储** —— 拨号方
    // 用 `client_only()`，它不持有服务端证书。
    let webtransport = needle.contains("webtransport");
    let with_webtransport = |b: swarmdrop_net::Builder, listening: bool| {
        if !webtransport {
            return b;
        }
        b.webtransport(if listening {
            WebTransportConfig::with_store(Arc::new(WebTransportMemoryCertificateStore::default()))
        } else {
            WebTransportConfig::client_only()
        })
    };

    let server = with_webtransport(Endpoint::builder().listen(vec![listen.parse()?]), true)
        .bind()
        .await?;
    // 拨号方不监听：拨号不需要 listener，也排除「其实是对端拨过来的」这种解释。
    let client = match with_webtransport(Endpoint::builder().listen(Vec::new()), false)
        .bind()
        .await
    {
        Ok(client) => client,
        Err(e) => {
            server.close().await;
            return Err(e.into());
        }
    };
    let router = Router::builder(server.clone()).accept(SINK, Sink).spawn();

    // **任何**失败（含超时）都要落到下面的收尾。三档在同一个进程里顺序跑，一档漏关就把
    // 活着的 actor 留给后面几档 —— 那正是本文件要消除的那类测量误差。
    let result = match tokio::time::timeout(
        PER_CASE_TIMEOUT,
        measure(&server, &client, needle, total),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => Err(format!("超时（>{PER_CASE_TIMEOUT:?}）").into()),
    };

    router.shutdown().await;
    server.close().await;
    client.close().await;
    result
}

/// 连上、开流、传 `total` 字节，并等对端确认**收全**。
async fn measure(
    server: &Endpoint,
    client: &Endpoint,
    needle: &str,
    total: usize,
) -> Result<Duration, Box<dyn Error>> {
    let addrs = wait_addrs(server, needle).await?;
    client
        .connect(NodeAddr::with_addrs(server.node_id(), addrs))
        .await?;
    let mut stream = client.open(server.node_id(), SINK).await?;

    let payload = vec![0xA5u8; 256 * 1024];
    let started = Instant::now();

    let mut left = total;
    while left > 0 {
        let n = left.min(payload.len());
        stream.write_all(&payload[..n]).await?;
        left -= n;
    }
    // 半关闭写侧 → 对端读到 EOF → 回 ack。计时必须包到 ack 为止，否则量的是
    // 「写进本端发送缓冲」的速度而不是链路吞吐。
    stream.close().await?;

    let mut ack = [0u8; 8];
    stream.read_exact(&mut ack).await?;
    let elapsed = started.elapsed();

    let received = u64::from_le_bytes(ack);
    if received != total as u64 {
        return Err(format!("对端只收到 {received} 字节，应为 {total}").into());
    }
    Ok(elapsed)
}

/// 等监听地址就绪，并**只返回匹配 `needle` 的那些**。
///
/// 端口 0 由 OS 分配、webrtc-direct 的 certhash 由 transport 就绪后回填，两者都必须
/// 等 watch 而不是读一次快照。
async fn wait_addrs(endpoint: &Endpoint, needle: &str) -> Result<Vec<Addr>, Box<dyn Error>> {
    let want_certhash = needle.contains("webrtc");
    let mut watcher = endpoint.watch_addrs();
    let addrs = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let listen = watcher.get().listen;
            let matched: Vec<Addr> = listen
                .into_iter()
                .filter(|a| {
                    let s = a.to_string();
                    s.contains(needle) && (!want_certhash || s.contains("/certhash/"))
                })
                .collect();
            if !matched.is_empty() {
                return Ok(matched);
            }
            // 别 `expect`：endpoint 的 actor 若在此时收摊，panic 会带走整个进程，
            // 后面几档一个都测不成。这里降级成本档失败，与其余错误路径一致。
            if watcher.updated().await.is_none() {
                return Err("endpoint 在等待监听地址期间关闭了");
            }
        }
    })
    .await??;
    Ok(addrs)
}
