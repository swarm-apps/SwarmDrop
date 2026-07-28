//! 浏览器后端：原生 `RTCPeerConnection`。
//!
//! 与 [native 侧](crate::backend::native) 实现同一个 [`Backend`] 契约，但底层完全不同——
//! webrtc-rs 编不到 wasm（它自带 UDP/SCTP/DTLS 栈），浏览器侧只能用 W3C API。
//!
//! # 三处与 native 的实质差异
//!
//! | | native | 浏览器 |
//! |---|---|---|
//! | 事件 | `async fn poll()` 事件流 | **回调**（`onicecandidate` 等），须用闭包推进队列 |
//! | 建连接 | `build().await` | 构造函数同步返回 |
//! | 网卡绑定 | 必须自己枚举（否则 host candidate 作废） | 浏览器全权代管，无从也无需干预 |
//!
//! # `Send`
//!
//! `Backend: Send` 是 libp2p 的硬约束，而 JS 对象都不是 `Send`。wasm 是单线程，
//! `SendWrapper` 在非创建线程访问时 panic，因此包起来是安全的——官方 `webrtc-websys`
//! 同款做法。

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

use self::muxer::{INIT_CHANNEL_LABEL, Muxer};
use super::{Backend, BackendError, BackendEvent};
use crate::config::Config;
use crate::protocol::MessageType;

mod data_channel;
pub(crate) mod direct;
mod muxer;

/// 排队中的操作。
///
/// 浏览器的 SDP 操作返回 Promise，而 [`Backend`] 接口是同步的；故把调用记下来，
/// 待 [`Backend::poll`] 驱动。与 native 侧同构。
enum Op {
    StartOffer,
    AcceptOffer(String),
    AcceptAnswer(String),
    AddCandidate(String),
}

/// 浏览器 `RTCPeerConnection` 后端。
pub struct WasmBackend {
    inner: SendWrapper<Inner>,
    /// `BackendEvent` 本身是 `Send`，无需包装。
    events_rx: mpsc::UnboundedReceiver<BackendEvent>,
}

struct Inner {
    pc: RtcPeerConnection,
    queued: VecDeque<Op>,
    /// 进行中的操作。同一时刻至多一个，保证 SDP 状态机的调用顺序。
    running: Option<LocalBoxFuture<'static, Result<(), BackendError>>>,
    events_tx: mpsc::UnboundedSender<BackendEvent>,
    /// 对端开来的 DataChannel，建连后交给 muxer。
    inbound_dc_rx: Option<mpsc::UnboundedReceiver<RtcDataChannel>>,
    /// 回调闭包。drop 掉它们 JS 侧回调就失效，且没有任何错误提示。
    _callbacks: Callbacks,
}

struct Callbacks {
    _onicecandidate: Closure<dyn FnMut(RtcPeerConnectionIceEvent)>,
    _onstatechange: Closure<dyn FnMut()>,
    _ondatachannel: Closure<dyn FnMut(RtcDataChannelEvent)>,
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

        Ok(Self {
            inner: SendWrapper::new(Inner {
                pc,
                queued: VecDeque::new(),
                running: None,
                events_tx,
                inbound_dc_rx: Some(dc_rx),
                _callbacks: Callbacks {
                    _onicecandidate: onicecandidate,
                    _onstatechange: onstatechange,
                    _ondatachannel: ondatachannel,
                },
            }),
            events_rx,
        })
    }

    /// 供 [`crate::Factory`] 使用的便捷构造。
    pub fn factory() -> crate::Factory {
        std::sync::Arc::new(|config: &Config| Ok(Box::new(Self::new(config)?) as Box<dyn Backend>))
    }
}

impl Inner {
    /// 把一个排队操作变成可驱动的 future。
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
        // 接收端只有一个，take 掉即表示所有权已交出——再次调用返回 None。
        let incoming = inner.inbound_dc_rx.take()?;
        Some(StreamMuxerBox::new(Muxer::new(inner.pc.clone(), incoming)))
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

/// 从 `createOffer`/`createAnswer` 的结果里取 SDP 文本。
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
