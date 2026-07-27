//! spike：webrtc-rs 0.20 的**完整 ICE agent** 能否与浏览器 `RTCPeerConnection` 打通 DataChannel。
//!
//! 这是「自研 libp2p WebRTC transport（browser ↔ NAT 后原生端）」方案的技术地基。
//! 不成立则整条路线不用往下走。
//!
//! # 要回答的三个问题
//!
//! 1. **能不能通** —— 浏览器 offer → webrtc-rs answer → DataChannel open → 双向消息。
//! 2. **是不是完整 ICE**（最关键）—— webrtc-rs 是否**主动向 STUN 发绑定请求并生成
//!    `typ srflx` candidate**。libp2p-webrtc 现在的 webrtc-direct 是 **ICE-lite**：
//!    只被动应答、不收集候选、SDP 确定性构造，因此打不了洞。若 0.20 能产出 srflx，
//!    就证明打洞所需的那一半能力在库里是现成的。
//! 3. **API 形态** —— 0.20 相对 0.17（libp2p-webrtc 当前所用）是大重构：
//!    `PeerConnectionEventHandler` trait 取代闭包回调、`DataChannel` 变
//!    `Arc<dyn DataChannel>` + `poll()` 事件流、自带 `webrtc::runtime` 抽象。
//!    顺带记录接入成本。
//!
//! # 跑法
//!
//! ```bash
//! cargo run          # 然后浏览器打开 http://127.0.0.1:8099
//! ```
//!
//! 信令走本机 HTTP（vanilla ICE，等 gathering 完成后一次性交换），**刻意不接 libp2p**——
//! 这里只验传输层地基，signaling 协议是下一阶段的事。

use std::sync::{Arc, Mutex};

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::Html,
    routing::{get, post},
};
use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::peer_connection::{
    MediaEngine, PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler,
    RTCConfigurationBuilder, RTCIceGatheringState, RTCIceServer, RTCPeerConnectionState,
    RTCSessionDescription, Registry, register_default_interceptors,
};
use webrtc::runtime::{Runtime, Sender, channel, default_runtime};

/// 公共 STUN。用它才能验证问题 2——ICE-lite 实现永远不会产出 srflx candidate。
const STUN_URL: &str = "stun:stun.l.google.com:19302";

const LISTEN: &str = "127.0.0.1:8099";

/// 建好的连接要存活，否则 handler 返回后 `PeerConnection` 被 drop、连接立刻断。
#[derive(Clone, Default)]
struct AppState {
    alive: Arc<Mutex<Vec<Arc<dyn PeerConnection>>>>,
}

struct SpikeHandler {
    runtime: Arc<dyn Runtime>,
    gather_done: Sender<()>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for SpikeHandler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        println!("[ice] gathering: {state:?}");
        if state == RTCIceGatheringState::Complete {
            let _ = self.gather_done.try_send(());
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        println!("[pc ] state: {state}");
    }

    async fn on_data_channel(&self, dc: Arc<dyn DataChannel>) {
        let runtime = self.runtime.clone();
        self.runtime.spawn(Box::pin(async move {
            let label = dc.label().await.unwrap_or_else(|_| "<?>".into());
            println!("[dc ] 收到 DataChannel: {label} (id={})", dc.id());

            // 吞吐统计：接收侧才是真实速率，发送侧只反映填缓冲的速度。
            let mut bench_bytes = 0usize;
            let mut bench_start: Option<std::time::Instant> = None;

            while let Some(event) = dc.poll().await {
                match event {
                    DataChannelEvent::OnOpen => {
                        println!("[dc ] open");
                        let dc = dc.clone();
                        runtime.spawn(Box::pin(async move {
                            let _ = dc.send_text("hello from webrtc-rs 0.20").await;
                        }));
                    }
                    DataChannelEvent::OnMessage(msg) => {
                        let n = msg.data.len();
                        // 大消息是吞吐测试的数据帧，只累加；小消息是控制/回显。
                        if n > 256 {
                            let t0 = *bench_start.get_or_insert_with(std::time::Instant::now);
                            let before = bench_bytes;
                            bench_bytes += n;
                            // 每收满 1 MiB 打一次进度——连接若中途 failed，最后一行
                            // 就是断点位置。
                            if bench_bytes / 1048576 != before / 1048576 {
                                let ms = t0.elapsed().as_secs_f64() * 1e3;
                                println!(
                                    "[bch] …{} MiB @ {ms:.0} ms ({:.1} MiB/s)",
                                    bench_bytes / 1048576,
                                    bench_bytes as f64 / 1048576.0 / (ms / 1e3)
                                );
                            }
                            continue;
                        }
                        let text = String::from_utf8_lossy(&msg.data).to_string();
                        if text == "bench-done" {
                            let ms = bench_start.take().map_or(0.0, |t| t.elapsed().as_secs_f64() * 1e3);
                            let mib = bench_bytes as f64 / 1048576.0;
                            println!(
                                "[bch] 接收 {mib:.1} MiB / {ms:.0} ms = {:.1} MiB/s",
                                mib / (ms / 1e3)
                            );
                            let summary = format!("bench-recv {mib:.1} MiB in {ms:.0} ms");
                            bench_bytes = 0;
                            let dc = dc.clone();
                            runtime.spawn(Box::pin(async move {
                                let _ = dc.send_text(&summary).await;
                            }));
                            continue;
                        }
                        // 反向传输：Rust → 浏览器。这个方向才用得上 0.20 的发送侧
                        // 背压（`send()` 在设了 limit 后阻塞），是 PR #817 的正题。
                        if let Some(n) = text.strip_prefix("send-me ") {
                            let total: usize = n.trim().parse().unwrap_or(8 << 20);
                            let dc = dc.clone();
                            runtime.spawn(Box::pin(async move {
                                const CHUNK: usize = 16 * 1024;
                                let payload = bytes::BytesMut::from(&vec![7u8; CHUNK][..]);
                                let t0 = std::time::Instant::now();
                                let mut sent = 0usize;
                                while sent < total {
                                    // 无 limit 时这行永不阻塞（等同浏览器 send）；
                                    // 设了 limit 就变成真背压。
                                    if let Err(e) = dc.send(payload.clone()).await {
                                        println!("[snd] ⛔ 中断于 {} MiB: {e}", sent / 1048576);
                                        return;
                                    }
                                    sent += CHUNK;
                                }
                                let ms = t0.elapsed().as_secs_f64() * 1e3;
                                let mib = sent as f64 / 1048576.0;
                                println!(
                                    "[snd] 发出 {mib:.1} MiB / {ms:.0} ms = {:.1} MiB/s",
                                    mib / (ms / 1e3)
                                );
                                let _ = dc
                                    .send_text(&format!("send-done {mib:.1} MiB in {ms:.0} ms"))
                                    .await;
                            }));
                            continue;
                        }
                        println!("[dc ] ← {text}");
                        let dc = dc.clone();
                        runtime.spawn(Box::pin(async move {
                            let _ = dc.send_text(&format!("echo: {text}")).await;
                        }));
                    }
                    DataChannelEvent::OnClose => {
                        println!("[dc ] closed");
                        break;
                    }
                    _ => {}
                }
            }
        }));
    }
}

/// ICE 要绑定的本地地址。
///
/// `SPIKE_BIND` 逗号分隔（如 `192.168.50.105,127.0.0.1`）；不设则退回 `0.0.0.0`——
/// 那正是上面注释里那个坑的复现路径，留着方便对照。
fn bind_addrs() -> Vec<String> {
    match std::env::var("SPIKE_BIND") {
        Ok(v) if !v.trim().is_empty() => v
            .split(',')
            .map(|s| format!("{}:0", s.trim()))
            .collect(),
        _ => {
            println!("[!! ] 未设 SPIKE_BIND，绑 0.0.0.0 —— host candidate 将不可用（见代码注释）");
            vec!["0.0.0.0:0".to_string()]
        }
    }
}

/// 读一个数值型环境变量（支持 `4194304` 或 `4m` / `256k` 后缀）。
fn env_num(key: &str) -> Option<usize> {
    let raw = std::env::var(key).ok()?;
    let s = raw.trim().to_lowercase();
    let (num, mul) = match s.strip_suffix('m') {
        Some(n) => (n, 1024 * 1024),
        None => match s.strip_suffix('k') {
            Some(n) => (n, 1024),
            None => (s.as_str(), 1),
        },
    };
    num.parse::<usize>().ok().map(|v| v * mul)
}

/// 把 SDP 里的 `a=candidate:` 行按 `typ` 归类打印——问题 2 的判据就在这。
fn report_candidates(tag: &str, sdp: &str) {
    let (mut host, mut srflx, mut relay, mut other) = (0, 0, 0, 0);
    for line in sdp.lines().filter(|l| l.starts_with("a=candidate:")) {
        match line {
            l if l.contains(" typ host") => host += 1,
            l if l.contains(" typ srflx") => srflx += 1,
            l if l.contains(" typ relay") => relay += 1,
            _ => other += 1,
        }
        println!("      {}", line.trim());
    }
    println!("[{tag}] candidates: host={host} srflx={srflx} relay={relay} other={other}");
    if tag == "ans" {
        if srflx > 0 {
            println!("[ans] ✅ 产出了 srflx —— webrtc-rs 0.20 是**完整 ICE agent**（会主动打 STUN）");
        } else {
            println!("[ans] ❌ 无 srflx —— 未做 STUN 绑定，打洞能力存疑（或网络阻断了 STUN）");
        }
    }
}

async fn signal(
    State(app): State<AppState>,
    Json(offer): Json<RTCSessionDescription>,
) -> Result<Json<RTCSessionDescription>, (StatusCode, String)> {
    build_answer(app, offer)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

async fn build_answer(
    app: AppState,
    offer: RTCSessionDescription,
) -> anyhow::Result<RTCSessionDescription> {
    println!("\n=== 新的 offer ===");
    report_candidates("off", &offer.sdp);

    let runtime =
        default_runtime().ok_or_else(|| anyhow::anyhow!("no async runtime found"))?;
    let (gather_done, mut gather_rx) = channel::<()>(1);

    let mut media_engine = MediaEngine::default();
    media_engine.register_default_codecs()?;
    let registry = register_default_interceptors(Registry::new(), &mut media_engine)?;

    let config = RTCConfigurationBuilder::new()
        .with_ice_servers(vec![RTCIceServer {
            urls: vec![STUN_URL.to_string()],
            ..Default::default()
        }])
        .build();

    let mut builder = PeerConnectionBuilder::new()
        .with_configuration(config)
        .with_media_engine(media_engine)
        .with_interceptor_registry(registry)
        .with_handler(Arc::new(SpikeHandler {
            runtime: runtime.clone(),
            gather_done,
        }))
        .with_runtime(runtime)
        // ⚠️ 坑：`with_udp_addrs(["0.0.0.0:0"])` **不会枚举网卡**，webrtc-rs 会把字面量
        // `0.0.0.0` 原样写进 host candidate —— 对端拿到一个不可连接的地址，于是 host
        // 路径整条作废、只能退到 srflx 走 NAT hairpin（实测吞吐掉到 ~0.6 MiB/s）。
        // 必须显式传本机可路由 IP。libp2p-webrtc 的 direct 模式是 ICE-lite + 确定性
        // SDP，不需要枚举网卡，所以这个坑在那边不存在，切完整 ICE 后才会暴露。
        .with_udp_addrs(bind_addrs());

    // 两个背压旋钮（0.20 新增，默认都不启用）：
    // - SPIKE_RWND        → SCTP 接收窗口（a_rwnd）。**默认 1 MiB**，正是浏览器全速
    //                       灌入时断连位置的量级，调大它是「接收侧断连」的对照变量。
    // - SPIKE_SEND_LIMIT  → 发送缓冲上限。默认 usize::MAX（无界），设了之后 `send()`
    //                       会阻塞到有空间（仿 tokio::mpsc::Sender::send）。
    if let Some(v) = env_num("SPIKE_RWND") {
        println!("[cfg] SCTP 接收窗口 = {v} B");
        builder = builder.with_sctp_receive_buffer_size(v as u32);
    }
    if let Some(v) = env_num("SPIKE_SEND_LIMIT") {
        println!("[cfg] 发送缓冲上限 = {v} B（send 将阻塞式背压）");
        builder = builder.with_data_channel_send_buffer_limit(v);
    }

    let pc = builder.build().await?;

    pc.set_remote_description(offer).await?;
    let answer = pc.create_answer(None).await?;
    pc.set_local_description(answer).await?;

    // vanilla ICE：等 gathering 完成，candidate 全部内联在 SDP 里，只交换一次。
    let _ = gather_rx.recv().await;

    let local = pc
        .local_description()
        .await
        .ok_or_else(|| anyhow::anyhow!("local_description 为空"))?;
    report_candidates("ans", &local.sdp);

    app.alive.lock().expect("mutex").push(Arc::new(pc));
    Ok(local)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let app = Router::new()
        .route("/", get(|| async { Html(include_str!("../static/index.html")) }))
        .route("/signal", post(signal))
        .with_state(AppState::default());

    println!("spike: webrtc-rs 0.20 ↔ 浏览器 ICE 验证");
    println!("打开 http://{LISTEN}\n");

    let listener = tokio::net::TcpListener::bind(LISTEN).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
