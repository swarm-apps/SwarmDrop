//! Native backend: `webrtc-rs` 0.20.
//!
//! The parameters and pitfalls below all come from spike measurements
//! (`dev-notes/research/2026-07-webrtc-native-ice.md`): 0.20 is a full ICE agent (it
//! actively queries STUN and produces srflx candidates), and the host path measured
//! 50 MiB/s.
//!
//! # Three settings that cannot be skipped
//!
//! | Setting | What the default gets wrong |
//! |---|---|
//! | `with_sctp_receive_buffer_size` | Defaults to 1 MiB, which a high-bandwidth LAN overruns within a few hundred milliseconds — **the connection drops rather than slowing down** |
//! | `with_data_channel_send_buffer_limit` | Unbounded by default, so a fast producer can exhaust memory; but below 4 MiB it starves the pipe |
//! | `with_udp_addrs` given concrete interface IPs | Passing `0.0.0.0` does not expand to interfaces; the host candidate is written as the literal `0.0.0.0`, which the remote cannot use |
//!
//! # Why no spawn is needed
//!
//! `webrtc-rs` carries its own runtime abstraction and manages its internal tasks; this
//! module only drives a handful of one-shot futures inside [`Backend::poll`] and catches
//! callback events over a channel. The crate therefore never calls `spawn` and needs no
//! runtime shim — but it **requires the caller to be inside a tokio (or smol) runtime**,
//! which `default_runtime()` relies on.

use std::collections::VecDeque;
use std::net::IpAddr;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures::channel::mpsc;
use futures::future::BoxFuture;
use futures::{FutureExt, StreamExt};
use webrtc::peer_connection::{
    MediaEngine, PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler,
    RTCConfigurationBuilder, RTCIceServer, RTCPeerConnectionIceEvent, RTCPeerConnectionState,
    RTCSessionDescription, Registry, register_default_interceptors,
};
use webrtc::runtime::default_runtime;

use libp2p_core::muxing::StreamMuxerBox;
use webrtc::data_channel::DataChannel;

use self::muxer::{INIT_CHANNEL_LABEL as MUXER_INIT_LABEL, Muxer};
use super::{Backend, BackendError, BackendEvent};
use crate::config::Config;
use crate::protocol::MessageType;

pub(crate) mod data_channel;
pub mod direct;
pub(crate) mod muxer;

/// SCTP receive window. The 1 MiB default drops the connection outright on a LAN (the
/// spike observed failures at 4 MiB).
const SCTP_RECEIVE_BUFFER: u32 = 8 * 1024 * 1024;

/// Send buffer limit. Unbounded by default; measurements showed 1 MiB halves throughput
/// while 4 MiB matches unbounded and caps memory.
const SEND_BUFFER_LIMIT: usize = 4 * 1024 * 1024;

/// A queued operation.
///
/// Constructing a `PeerConnection` and each of its operations are async, while the
/// [`Backend`] interface is synchronous; the call is therefore recorded and driven later
/// by [`Backend::poll`]. That keeps the complexity inside the backend instead of turning
/// the trait async.
#[derive(Debug)]
enum Op {
    StartOffer,
    AcceptOffer(String),
    AcceptAnswer(String),
    AddCandidate(String),
}

/// The `webrtc-rs` backend.
pub struct NativeBackend {
    state: State,
    queued: VecDeque<Op>,
    /// The operation in flight. At most one at a time, which preserves the SDP state
    /// machine's call ordering.
    running: Option<BoxFuture<'static, Result<(), BackendError>>>,
    events_tx: mpsc::UnboundedSender<BackendEvent>,
    events_rx: mpsc::UnboundedReceiver<BackendEvent>,
    /// The DataChannel opened by the remote; handed to the muxer along with the
    /// PeerConnection once connected.
    inbound_dc_rx: Option<mpsc::UnboundedReceiver<Arc<dyn DataChannel>>>,
}

enum State {
    /// The `PeerConnection` is still being built (`build()` is async).
    Building(BoxFuture<'static, Result<Arc<dyn PeerConnection>, BackendError>>),
    Ready(Arc<dyn PeerConnection>),
    Failed,
}

impl std::fmt::Debug for NativeBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = match self.state {
            State::Building(_) => "building",
            State::Ready(_) => "ready",
            State::Failed => "failed",
        };
        f.debug_struct("NativeBackend")
            .field("state", &state)
            .field("queued", &self.queued.len())
            .field("running", &self.running.is_some())
            .finish()
    }
}

impl NativeBackend {
    /// Constructs synchronously; the real `PeerConnection` is built on the first
    /// [`Backend::poll`].
    pub fn new(config: &Config) -> Self {
        let (events_tx, events_rx) = mpsc::unbounded();
        let (inbound_dc_tx, inbound_dc_rx) = mpsc::unbounded();
        let building =
            build_peer_connection(config.clone(), events_tx.clone(), inbound_dc_tx).boxed();
        Self {
            state: State::Building(building),
            queued: VecDeque::new(),
            running: None,
            events_tx,
            events_rx,
            inbound_dc_rx: Some(inbound_dc_rx),
        }
    }

    /// Convenience constructor for use by [`crate::Factory`].
    pub fn factory() -> crate::Factory {
        Arc::new(|config: &Config| Ok(Box::new(Self::new(config)) as Box<dyn Backend>))
    }

    fn fail(&mut self, err: BackendError) {
        self.state = State::Failed;
        self.queued.clear();
        self.running = None;
        let _ = self
            .events_tx
            .unbounded_send(BackendEvent::Failed(err.to_string()));
    }

    /// Turns a queued operation into a drivable future.
    fn spawn_op(&mut self, pc: Arc<dyn PeerConnection>, op: Op) {
        let tx = self.events_tx.clone();
        self.running = Some(
            async move {
                match op {
                    Op::StartOffer => {
                        // spec 步骤 4：offer 之前必须先建 `init` DataChannel。
                        // 这条不承载数据，有序与否本无影响；用同一个构造器是为了让
                        // 「本 crate 从不传 `None`」成为无例外的规则——默认值是个陷阱，
                        // 见 [`muxer::ordered_reliable`]。
                        pc.create_data_channel(MUXER_INIT_LABEL, Some(muxer::ordered_reliable()))
                            .await
                            .map_err(|e| BackendError::new(format!("创建 init 通道失败：{e}")))?;
                        let offer = pc
                            .create_offer(None)
                            .await
                            .map_err(|e| BackendError::new(format!("创建 offer 失败：{e}")))?;
                        let sdp = offer.sdp.clone();
                        pc.set_local_description(offer)
                            .await
                            .map_err(|e| BackendError::new(format!("设置本地 offer 失败：{e}")))?;
                        let _ = tx.unbounded_send(BackendEvent::LocalDescription {
                            ty: MessageType::SdpOffer,
                            sdp,
                        });
                    }
                    Op::AcceptOffer(sdp) => {
                        let desc = RTCSessionDescription::offer(sdp)
                            .map_err(|e| BackendError::new(format!("解析远端 offer 失败：{e}")))?;
                        pc.set_remote_description(desc)
                            .await
                            .map_err(|e| BackendError::new(format!("设置远端 offer 失败：{e}")))?;
                        let answer = pc
                            .create_answer(None)
                            .await
                            .map_err(|e| BackendError::new(format!("创建 answer 失败：{e}")))?;
                        let sdp = answer.sdp.clone();
                        pc.set_local_description(answer)
                            .await
                            .map_err(|e| BackendError::new(format!("设置本地 answer 失败：{e}")))?;
                        let _ = tx.unbounded_send(BackendEvent::LocalDescription {
                            ty: MessageType::SdpAnswer,
                            sdp,
                        });
                    }
                    Op::AcceptAnswer(sdp) => {
                        let desc = RTCSessionDescription::answer(sdp)
                            .map_err(|e| BackendError::new(format!("解析远端 answer 失败：{e}")))?;
                        pc.set_remote_description(desc)
                            .await
                            .map_err(|e| BackendError::new(format!("设置远端 answer 失败：{e}")))?;
                    }
                    Op::AddCandidate(json) => {
                        let init = serde_json::from_str(&json).map_err(|e| {
                            BackendError::new(format!("解析远端 candidate 失败：{e}"))
                        })?;
                        pc.add_ice_candidate(init).await.map_err(|e| {
                            BackendError::new(format!("添加远端 candidate 失败：{e}"))
                        })?;
                    }
                }
                Ok(())
            }
            .boxed(),
        );
    }
}

impl Backend for NativeBackend {
    fn start_offer(&mut self) -> Result<(), BackendError> {
        self.queued.push_back(Op::StartOffer);
        Ok(())
    }

    fn accept_offer(&mut self, sdp: &str) -> Result<(), BackendError> {
        self.queued.push_back(Op::AcceptOffer(sdp.to_string()));
        Ok(())
    }

    fn accept_answer(&mut self, sdp: &str) -> Result<(), BackendError> {
        self.queued.push_back(Op::AcceptAnswer(sdp.to_string()));
        Ok(())
    }

    fn add_remote_candidate(&mut self, json: &str) -> Result<(), BackendError> {
        self.queued.push_back(Op::AddCandidate(json.to_string()));
        Ok(())
    }

    fn take_muxer(&mut self) -> Option<StreamMuxerBox> {
        let State::Ready(pc) = &self.state else {
            return None;
        };
        // 接收端只有一个，take 掉即表示所有权已交出——再次调用返回 None。
        let incoming = self.inbound_dc_rx.take()?;
        Some(StreamMuxerBox::new(Muxer::new(pc.clone(), incoming)))
    }

    fn poll(&mut self, cx: &mut Context<'_>) -> Poll<BackendEvent> {
        // 1. 等 PeerConnection 建好。
        if let State::Building(fut) = &mut self.state {
            match fut.poll_unpin(cx) {
                Poll::Ready(Ok(pc)) => self.state = State::Ready(pc),
                Poll::Ready(Err(e)) => {
                    self.fail(e);
                    return self
                        .events_rx
                        .poll_next_unpin(cx)
                        .map(|e| e.unwrap_or_else(|| BackendEvent::Failed("后端已关闭".into())));
                }
                Poll::Pending => return Poll::Pending,
            }
        }

        // 2. 驱动当前操作，完成后取下一个。顺序执行是必需的——SDP 状态机不接受乱序。
        loop {
            if let Some(fut) = &mut self.running {
                match fut.poll_unpin(cx) {
                    Poll::Ready(Ok(())) => self.running = None,
                    Poll::Ready(Err(e)) => {
                        self.fail(e);
                        break;
                    }
                    Poll::Pending => break,
                }
            }
            let State::Ready(pc) = &self.state else { break };
            let Some(op) = self.queued.pop_front() else {
                break;
            };
            let pc = pc.clone();
            self.spawn_op(pc, op);
        }

        // 3. 交出回调与操作产生的事件。
        match self.events_rx.poll_next_unpin(cx) {
            Poll::Ready(Some(event)) => Poll::Ready(event),
            // 发送端由自身持有，正常情况下不会走到这里。
            Poll::Ready(None) => Poll::Pending,
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Forwards callback events onto the channel.
#[derive(Clone)]
struct EventForwarder {
    tx: mpsc::UnboundedSender<BackendEvent>,
    dc_tx: mpsc::UnboundedSender<Arc<dyn DataChannel>>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for EventForwarder {
    async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
        // spec 步骤 7：候选**陆续**产生就陆续发出（trickle ICE），不等收集完毕。
        match event.candidate.to_json() {
            Ok(init) => match serde_json::to_string(&init) {
                Ok(json) => {
                    let _ = self.tx.unbounded_send(BackendEvent::LocalCandidate(json));
                }
                Err(e) => tracing::warn!("序列化本地 candidate 失败：{e}"),
            },
            Err(e) => tracing::warn!("转换本地 candidate 失败：{e}"),
        }
    }

    async fn on_data_channel(&self, dc: Arc<dyn DataChannel>) {
        // 交给 muxer 决定去留：init 通道会在那里被跳过（它不是数据流）。
        let _ = self.dc_tx.unbounded_send(dc);
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        let event = match state {
            RTCPeerConnectionState::Connected => BackendEvent::Connected,
            RTCPeerConnectionState::Failed => {
                BackendEvent::Failed("WebRTC 连接进入 failed 状态".into())
            }
            // Disconnected 可能自行恢复，不当作终态。
            _ => return,
        };
        let _ = self.tx.unbounded_send(event);
    }
}

/// Builds the `PeerConnection` asynchronously.
async fn build_peer_connection(
    config: Config,
    events_tx: mpsc::UnboundedSender<BackendEvent>,
    dc_tx: mpsc::UnboundedSender<Arc<dyn DataChannel>>,
) -> Result<Arc<dyn PeerConnection>, BackendError> {
    let runtime = default_runtime().ok_or_else(|| {
        BackendError::new("未检测到 async 运行时：native 后端需在 tokio 或 smol 中运行")
    })?;

    let mut media_engine = MediaEngine::default();
    media_engine
        .register_default_codecs()
        .map_err(|e| BackendError::new(format!("注册编解码器失败：{e}")))?;
    let registry = register_default_interceptors(Registry::new(), &mut media_engine)
        .map_err(|e| BackendError::new(format!("注册 interceptor 失败：{e}")))?;

    let rtc_config = RTCConfigurationBuilder::new()
        .with_ice_servers(vec![RTCIceServer {
            urls: config.stun_servers().to_vec(),
            ..Default::default()
        }])
        .build();

    let pc = PeerConnectionBuilder::new()
        .with_configuration(rtc_config)
        .with_media_engine(media_engine)
        .with_interceptor_registry(registry)
        .with_handler(Arc::new(EventForwarder {
            tx: events_tx,
            dc_tx,
        }))
        .with_runtime(runtime)
        .with_udp_addrs(bind_addrs(&config))
        .with_sctp_receive_buffer_size(SCTP_RECEIVE_BUFFER)
        .with_data_channel_send_buffer_limit(SEND_BUFFER_LIMIT)
        .build()
        .await
        .map_err(|e| BackendError::new(format!("构造 PeerConnection 失败：{e}")))?;

    Ok(Arc::new(pc))
}

/// The local addresses ICE should bind to.
///
/// **Never pass `0.0.0.0`**: webrtc-rs does not expand it into interfaces; it writes the
/// literal into the host candidate, so the remote receives an unconnectable address, the
/// entire host path is void, and traffic falls back to srflx through NAT hairpinning
/// (measured in a spike: throughput dropped from 50 MiB/s to 0.6 MiB/s).
///
/// This pitfall does not exist in webrtc-direct mode (ICE-lite plus a deterministic SDP
/// needs no interface enumeration); it only surfaces once full ICE is in play.
fn bind_addrs(config: &Config) -> Vec<String> {
    if !config.udp_bind_addrs().is_empty() {
        return config
            .udp_bind_addrs()
            .iter()
            .map(|a| a.to_string())
            .collect();
    }
    let mut addrs: Vec<String> = if_addrs::get_if_addrs()
        .map(|ifaces| {
            ifaces
                .into_iter()
                // 只要 IPv4：webrtc-rs 的 IPv6 支持面与 STUN 可达性都更不确定，
                // 而 IPv4 已足够覆盖打洞场景。
                .filter(|i| matches!(i.addr.ip(), IpAddr::V4(_)))
                .map(|i| format!("{}:0", i.addr.ip()))
                .collect()
        })
        .unwrap_or_default();
    if addrs.is_empty() {
        // 枚举失败时退回回环，至少同机可用，并留痕以便排查。
        tracing::warn!("未能枚举本机网卡，ICE 仅绑回环地址");
        addrs.push("127.0.0.1:0".to_string());
    }
    addrs
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 这是 spike 里踩过的坑：绑 `0.0.0.0` 会让 host candidate 变成不可用的字面量，
    /// 把 host 路径整条废掉。默认必须枚举出真实网卡地址。
    #[test]
    fn default_bind_addrs_are_concrete_not_wildcard() {
        let addrs = bind_addrs(&Config::default());
        assert!(!addrs.is_empty());
        for a in &addrs {
            assert!(!a.starts_with("0.0.0.0"), "绑定地址不能是通配符，实得 {a}");
        }
    }

    #[test]
    fn explicit_bind_addrs_win() {
        let config = Config::default().with_udp_bind_addrs(["127.0.0.1:1234".parse().unwrap()]);
        assert_eq!(bind_addrs(&config), ["127.0.0.1:1234"]);
    }

    /// 端到端走一遍 spec 步骤 4：建 init 通道 → 造 offer → 设为本地描述。
    /// 少了 init 通道 SDP 里不带 ICE 信息，这个测试能钉住那条要求。
    #[tokio::test]
    async fn start_offer_yields_sdp_with_ice() {
        let mut backend = NativeBackend::new(&Config::default());
        backend.start_offer().unwrap();

        let sdp = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let event = std::future::poll_fn(|cx| backend.poll(cx)).await;
                match event {
                    BackendEvent::LocalDescription { ty, sdp } => {
                        assert_eq!(ty, MessageType::SdpOffer);
                        return sdp;
                    }
                    BackendEvent::Failed(e) => panic!("后端失败：{e}"),
                    _ => continue,
                }
            }
        })
        .await
        .expect("10s 内应产出 offer");

        assert!(sdp.contains("v=0"), "应是合法 SDP");
        assert!(
            sdp.contains("a=ice-ufrag") && sdp.contains("a=ice-pwd"),
            "SDP 必须带 ICE 凭据——没有就说明 init DataChannel 没建成"
        );
        assert!(
            sdp.contains("m=application"),
            "init DataChannel 应在 SDP 里体现为 application m-line"
        );
    }

    /// 完整 ICE agent 的判据：会主动向 STUN 发绑定请求并产出 srflx 候选。
    /// ICE-lite 实现（libp2p 现有的 webrtc-direct）永远产不出这个。
    #[tokio::test]
    async fn gathers_candidates() {
        let mut backend = NativeBackend::new(&Config::default());
        backend.start_offer().unwrap();

        let got = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            let mut candidates = Vec::new();
            loop {
                match std::future::poll_fn(|cx| backend.poll(cx)).await {
                    BackendEvent::LocalCandidate(json) => {
                        candidates.push(json);
                        return candidates;
                    }
                    BackendEvent::Failed(e) => panic!("后端失败：{e}"),
                    _ => continue,
                }
            }
        })
        .await
        .expect("10s 内应产出至少一个候选");

        // 候选必须是 W3C camelCase JSON（spec 要求 RTCIceCandidate.toJSON() 的字符串），
        // 否则 js-libp2p 解不出来。
        let parsed: serde_json::Value = serde_json::from_str(&got[0]).expect("应是合法 JSON");
        assert!(
            parsed.get("candidate").is_some(),
            "缺 candidate 字段：{}",
            got[0]
        );
        assert!(
            parsed.get("sdpMLineIndex").is_some() || parsed.get("sdpMid").is_some(),
            "缺 sdpMLineIndex/sdpMid：{}",
            got[0]
        );
    }
}
