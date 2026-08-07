//! Browser backend: the native `RTCPeerConnection`.
//!
//! Implements the same [`Backend`] contract as the [native side](crate::backend::native),
//! but on a completely different foundation — webrtc-rs does not compile to wasm (it ships
//! its own UDP/SCTP/DTLS stack), so the browser side can only use the W3C API.
//!
//! # Three substantive differences from native
//!
//! | | Native | Browser |
//! |---|---|---|
//! | Events | an `async fn poll()` event stream | **callbacks** (`onicecandidate` and friends), which must push into a queue from a closure |
//! | Constructing the connection | `build().await` | the constructor returns synchronously |
//! | Interface binding | must be enumerated by hand (otherwise the host candidate is void) | fully managed by the browser; neither possible nor necessary to influence |
//!
//! # `Send`
//!
//! `Backend: Send` is a hard libp2p constraint, and no JS object is `Send`. wasm is
//! single-threaded and `SendWrapper` panics when accessed from another thread, which makes
//! wrapping sound — the same approach the official `webrtc-websys` takes.

use std::collections::VecDeque;
use std::task::{Context, Poll};

use futures::channel::mpsc;
use futures::future::LocalBoxFuture;
use futures::{FutureExt, StreamExt};
use libp2p_core::muxing::StreamMuxerBox;
use send_wrapper::SendWrapper;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    RtcConfiguration, RtcDataChannel, RtcDataChannelEvent, RtcIceCandidateInit, RtcPeerConnection,
    RtcPeerConnectionIceEvent, RtcPeerConnectionState, RtcSdpType, RtcSessionDescriptionInit,
};

use self::callbacks::JsCallbacks;
use self::muxer::{INIT_CHANNEL_LABEL, Muxer};
use super::{Backend, BackendError, BackendEvent};
use crate::config::Config;
use crate::protocol::MessageType;

mod callbacks;
mod data_channel;
pub(crate) mod direct;
mod muxer;

/// A queued operation.
///
/// The browser's SDP operations return Promises while the [`Backend`] interface is
/// synchronous, so the call is recorded and driven later by [`Backend::poll`] — the same
/// shape as on the native side.
enum Op {
    StartOffer,
    AcceptOffer(String),
    AcceptAnswer(String),
    AddCandidate(String),
}

/// Browser `RTCPeerConnection` backend.
pub struct WasmBackend {
    inner: SendWrapper<Inner>,
    /// `BackendEvent` is itself `Send`, so no wrapping is needed.
    events_rx: mpsc::UnboundedReceiver<BackendEvent>,
}

struct Inner {
    pc: RtcPeerConnection,
    queued: VecDeque<Op>,
    /// The operation in flight. At most one at a time, which preserves the SDP state
    /// machine's call ordering.
    running: Option<LocalBoxFuture<'static, Result<(), BackendError>>>,
    events_tx: mpsc::UnboundedSender<BackendEvent>,
    /// Everything handed to the data plane once the connection succeeds. `None` = already
    /// handed over.
    handover: Option<Handover>,
}

/// The things handed to the data plane together with [`Backend::take_muxer`].
///
/// They must live and die together, hence a single `Option`: this type only lives until the
/// signaling session ends (the moment `Action::Connected` fires, `connection_keep_alive()`
/// flips to false and the signaling connection on that relay is closed), while the data
/// plane must live much longer. Leave either one behind and it dies before the connection
/// does — the callbacks fail silently, and the receiving side gets an immediate "connection
/// closed" out of `poll_inbound`.
struct Handover {
    /// The DataChannel opened by the remote.
    incoming: mpsc::UnboundedReceiver<RtcDataChannel>,
    /// The callback closures registered on `pc`; see [`muxer::Inner::_callbacks`].
    callbacks: JsCallbacks<RtcPeerConnection>,
}

impl std::fmt::Debug for WasmBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmBackend")
            .field("queued", &self.inner.queued.len())
            .field("running", &self.inner.running.is_some())
            .finish_non_exhaustive()
    }
}

impl WasmBackend {
    pub fn new(config: &Config) -> Result<Self, BackendError> {
        let pc = new_peer_connection(config)?;
        let (events_tx, events_rx) = mpsc::unbounded();
        let (dc_tx, dc_rx) = mpsc::unbounded();

        let onicecandidate = {
            let tx = events_tx.clone();
            Closure::wrap(Box::new(move |ev: RtcPeerConnectionIceEvent| {
                // candidate 为 None 表示收集结束，不是候选本身。
                let Some(candidate) = ev.candidate() else {
                    return;
                };
                // spec 步骤 7 要求发 `RTCIceCandidate.toJSON()` 的字符串。
                match js_sys::JSON::stringify(&candidate.to_json()) {
                    Ok(json) => {
                        let _ = tx.unbounded_send(BackendEvent::LocalCandidate(json.into()));
                    }
                    Err(e) => tracing::warn!("序列化本地 candidate 失败：{e:?}"),
                }
            }) as Box<dyn FnMut(RtcPeerConnectionIceEvent)>)
        };

        let onstatechange = {
            let tx = events_tx.clone();
            let pc = pc.clone();
            Closure::wrap(Box::new(move || {
                let event = match pc.connection_state() {
                    RtcPeerConnectionState::Connected => BackendEvent::Connected,
                    RtcPeerConnectionState::Failed => {
                        BackendEvent::Failed("WebRTC 连接进入 failed 状态".into())
                    }
                    // Disconnected 可能自行恢复，不当作终态。
                    _ => return,
                };
                let _ = tx.unbounded_send(event);
            }) as Box<dyn FnMut()>)
        };

        let ondatachannel = Closure::wrap(Box::new(move |ev: RtcDataChannelEvent| {
            // 交给 muxer 决定去留：init 通道会在那里被跳过（它不是数据流）。
            let _ = dc_tx.unbounded_send(ev.channel());
        }) as Box<dyn FnMut(RtcDataChannelEvent)>);

        pc.set_onicecandidate(Some(onicecandidate.as_ref().unchecked_ref()));
        pc.set_onconnectionstatechange(Some(onstatechange.as_ref().unchecked_ref()));
        pc.set_ondatachannel(Some(ondatachannel.as_ref().unchecked_ref()));

        let callbacks = JsCallbacks::new(
            pc.clone(),
            |pc: &RtcPeerConnection| {
                pc.set_onicecandidate(None);
                pc.set_onconnectionstatechange(None);
                pc.set_ondatachannel(None);
            },
            (onicecandidate, onstatechange, ondatachannel),
        );

        Ok(Self {
            inner: SendWrapper::new(Inner {
                pc,
                queued: VecDeque::new(),
                running: None,
                events_tx,
                handover: Some(Handover {
                    incoming: dc_rx,
                    callbacks,
                }),
            }),
            events_rx,
        })
    }

    /// Convenience constructor for use by [`crate::Factory`].
    pub fn factory() -> crate::Factory {
        std::sync::Arc::new(|config: &Config| Ok(Box::new(Self::new(config)?) as Box<dyn Backend>))
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        // 信令会话失败（或根本没走到建连）时，这条 `RTCPeerConnection` 无人接手，
        // 而浏览器不会回收一条还在跑 ICE 的连接——必须显式关。已移交时**绝不能关**：
        // 那正是刚建好的连接，此后归数据面管（见 [`Handover`]）。
        if self.handover.is_some() {
            self.pc.close();
        }
    }
}

impl Inner {
    /// Turns a queued operation into a drivable future.
    fn start_op(&mut self, op: Op) {
        let pc = self.pc.clone();
        let tx = self.events_tx.clone();
        self.running = Some(
            async move {
                match op {
                    Op::StartOffer => {
                        // spec 步骤 4：offer 之前必须先建 `init` DataChannel，
                        // 否则 SDP 里不带 ICE 信息。
                        pc.create_data_channel(INIT_CHANNEL_LABEL);
                        let offer = JsFuture::from(pc.create_offer())
                            .await
                            .map_err(|e| js_err("创建 offer", e))?;
                        let sdp = sdp_of(&offer)?;
                        let desc = describe(RtcSdpType::Offer, &sdp);
                        JsFuture::from(pc.set_local_description(&desc))
                            .await
                            .map_err(|e| js_err("设置本地 offer", e))?;
                        let _ = tx.unbounded_send(BackendEvent::LocalDescription {
                            ty: MessageType::SdpOffer,
                            sdp,
                        });
                    }
                    Op::AcceptOffer(sdp) => {
                        let desc = describe(RtcSdpType::Offer, &sdp);
                        JsFuture::from(pc.set_remote_description(&desc))
                            .await
                            .map_err(|e| js_err("设置远端 offer", e))?;
                        let answer = JsFuture::from(pc.create_answer())
                            .await
                            .map_err(|e| js_err("创建 answer", e))?;
                        let sdp = sdp_of(&answer)?;
                        let desc = describe(RtcSdpType::Answer, &sdp);
                        JsFuture::from(pc.set_local_description(&desc))
                            .await
                            .map_err(|e| js_err("设置本地 answer", e))?;
                        let _ = tx.unbounded_send(BackendEvent::LocalDescription {
                            ty: MessageType::SdpAnswer,
                            sdp,
                        });
                    }
                    Op::AcceptAnswer(sdp) => {
                        let desc = describe(RtcSdpType::Answer, &sdp);
                        JsFuture::from(pc.set_remote_description(&desc))
                            .await
                            .map_err(|e| js_err("设置远端 answer", e))?;
                    }
                    Op::AddCandidate(json) => {
                        let parsed = js_sys::JSON::parse(&json)
                            .map_err(|e| js_err("解析远端 candidate", e))?;
                        let init: RtcIceCandidateInit = parsed.unchecked_into();
                        JsFuture::from(
                            pc.add_ice_candidate_with_opt_rtc_ice_candidate_init(Some(&init)),
                        )
                        .await
                        .map_err(|e| js_err("添加远端 candidate", e))?;
                    }
                }
                Ok(())
            }
            .boxed_local(),
        );
    }
}

impl Backend for WasmBackend {
    fn start_offer(&mut self) -> Result<(), BackendError> {
        self.inner.queued.push_back(Op::StartOffer);
        Ok(())
    }

    fn accept_offer(&mut self, sdp: &str) -> Result<(), BackendError> {
        self.inner
            .queued
            .push_back(Op::AcceptOffer(sdp.to_string()));
        Ok(())
    }

    fn accept_answer(&mut self, sdp: &str) -> Result<(), BackendError> {
        self.inner
            .queued
            .push_back(Op::AcceptAnswer(sdp.to_string()));
        Ok(())
    }

    fn add_remote_candidate(&mut self, json: &str) -> Result<(), BackendError> {
        self.inner
            .queued
            .push_back(Op::AddCandidate(json.to_string()));
        Ok(())
    }

    fn take_muxer(&mut self) -> Option<StreamMuxerBox> {
        let inner = &mut *self.inner;
        // take 掉即表示所有权已交出——再次调用返回 None。
        let handover = inner.handover.take()?;
        // 打洞路径没有 Noise 握手，也就没有可协商的消息尺寸上限，取默认值。
        Some(StreamMuxerBox::new(Muxer::new(
            inner.pc.clone(),
            handover.incoming,
            libp2p_webrtc_utils::StreamConfig::default(),
            handover.callbacks,
        )))
    }

    fn poll(&mut self, cx: &mut Context<'_>) -> Poll<BackendEvent> {
        let inner = &mut *self.inner;

        // 顺序执行排队的操作——SDP 状态机不接受乱序。
        loop {
            if let Some(fut) = &mut inner.running {
                match fut.poll_unpin(cx) {
                    Poll::Ready(Ok(())) => inner.running = None,
                    Poll::Ready(Err(e)) => {
                        inner.queued.clear();
                        inner.running = None;
                        let _ = inner
                            .events_tx
                            .unbounded_send(BackendEvent::Failed(e.to_string()));
                        break;
                    }
                    Poll::Pending => break,
                }
            }
            let Some(op) = inner.queued.pop_front() else {
                break;
            };
            inner.start_op(op);
        }

        match self.events_rx.poll_next_unpin(cx) {
            Poll::Ready(Some(event)) => Poll::Ready(event),
            // 发送端由自身持有，正常情况下不会走到这里。
            Poll::Ready(None) | Poll::Pending => Poll::Pending,
        }
    }
}

fn new_peer_connection(config: &Config) -> Result<RtcPeerConnection, BackendError> {
    let ice_servers = js_sys::Array::new();
    for url in config.stun_servers() {
        let server = js_sys::Object::new();
        js_sys::Reflect::set(&server, &JsValue::from_str("urls"), &JsValue::from_str(url))
            .map_err(|e| js_err("设置 STUN 地址", e))?;
        ice_servers.push(&server);
    }

    let rtc_config = RtcConfiguration::new();
    rtc_config.set_ice_servers(&ice_servers);

    RtcPeerConnection::new_with_configuration(&rtc_config)
        .map_err(|e| js_err("构造 RTCPeerConnection", e))
}

/// Extracts the SDP text from the result of `createOffer`/`createAnswer`.
fn sdp_of(desc: &JsValue) -> Result<String, BackendError> {
    js_sys::Reflect::get(desc, &JsValue::from_str("sdp"))
        .ok()
        .and_then(|v| v.as_string())
        .ok_or_else(|| BackendError::new("SDP 描述里没有 sdp 字段"))
}

fn describe(ty: RtcSdpType, sdp: &str) -> RtcSessionDescriptionInit {
    let desc = RtcSessionDescriptionInit::new(ty);
    desc.set_sdp(sdp);
    desc
}

fn js_err(what: &str, e: JsValue) -> BackendError {
    BackendError::new(format!("{what}失败：{e:?}"))
}
