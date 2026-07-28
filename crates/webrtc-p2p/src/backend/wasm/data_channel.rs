//! 浏览器 `RTCDataChannel` → `AsyncRead + AsyncWrite` 适配。
//!
//! 与 [native 侧](crate::backend::native) 同名模块解决同一个问题，但形状不同：浏览器侧
//! 是**回调驱动**（`onmessage` / `onclose`），不是可 await 的事件流。故用闭包把消息推进
//! 队列，poll 时从队列取。
//!
//! # 两条约束
//!
//! 1. **闭包必须被持有**。`Closure::wrap` 出来的对象一旦 drop，JS 侧的回调就失效——
//!    消息会静默丢失，且没有任何错误提示。故它们与队列同寿。
//! 2. **必须 `Clone`**（`Stream<T>` 的要求），且 clone 出的副本要共享同一条通道与同一份
//!    队列，因此状态全部收在 `Rc<RefCell<_>>` 里。
//!
//! `SendWrapper` 由上层（[`super::WasmBackend`]）统一施加：本类型自身不跨线程流转，
//! 是被 `Stream<T>` 持有后随连接一起被包起来的。

use std::cell::RefCell;
use std::io;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use bytes::BytesMut;
use futures::{AsyncRead, AsyncWrite};
use wasm_bindgen::prelude::*;
use web_sys::{MessageEvent, RtcDataChannel, RtcDataChannelState, RtcDataChannelType};

/// 发送缓冲上限。超过就背压，等浏览器排空到低水位再继续。
///
/// 没有它，快生产者能把浏览器的发送缓冲无限撑大——spike 实测过同类失败：8 MiB 突发
/// 会把链路打满，连接直接断掉而非降速。native 侧靠「等上一条 send 完成」实现背压，
/// 浏览器侧只能靠 bufferedAmount 自己数。
const MAX_BUFFERED_AMOUNT: u32 = 1024 * 1024;

/// 低水位。降到这里才唤醒写侧——留出一半余量，避免在阈值附近反复抖动。
const LOW_WATER_MARK: u32 = MAX_BUFFERED_AMOUNT / 2;

/// 把浏览器 `RTCDataChannel` 包装成字节流。
///
/// `Clone` 语义是「共享同一条通道」，不是复制。
#[derive(Clone)]
pub(crate) struct PollDataChannel {
    dc: RtcDataChannel,
    shared: Rc<RefCell<Shared>>,
    /// 回调闭包。放这里只为让它们活到通道关闭——drop 掉回调就失效了。
    _callbacks: Rc<Callbacks>,
}

#[derive(Default)]
struct Shared {
    /// 已到达但尚未被读走的字节。
    read_buf: BytesMut,
    /// 对端已关闭，读侧到达 EOF。
    eof: bool,
    /// 通道出错。
    error: Option<String>,
    /// 等待可读的任务。
    read_waker: Option<Waker>,
    /// 因发送缓冲满而等待的任务。
    write_waker: Option<Waker>,
}

impl Shared {
    fn wake_read(&mut self) {
        if let Some(w) = self.read_waker.take() {
            w.wake();
        }
    }

    fn wake_write(&mut self) {
        if let Some(w) = self.write_waker.take() {
            w.wake();
        }
    }
}

struct Callbacks {
    _onmessage: Closure<dyn FnMut(MessageEvent)>,
    _onclose: Closure<dyn FnMut()>,
    _onerror: Closure<dyn FnMut(JsValue)>,
    _onbufferedamountlow: Closure<dyn FnMut()>,
}

impl PollDataChannel {
    pub(crate) fn new(dc: RtcDataChannel) -> Self {
        // 必须收 ArrayBuffer：默认是 Blob，取字节要再走一次异步读取，凭空多一层状态。
        dc.set_binary_type(RtcDataChannelType::Arraybuffer);

        let shared = Rc::new(RefCell::new(Shared::default()));

        let onmessage = {
            let shared = shared.clone();
            Closure::wrap(Box::new(move |ev: MessageEvent| {
                let data = ev.data();
                let bytes = js_sys::Uint8Array::new(&data).to_vec();
                let mut s = shared.borrow_mut();
                s.read_buf.extend_from_slice(&bytes);
                s.wake_read();
            }) as Box<dyn FnMut(MessageEvent)>)
        };
        let onclose = {
            let shared = shared.clone();
            Closure::wrap(Box::new(move || {
                let mut s = shared.borrow_mut();
                s.eof = true;
                s.wake_read();
                // 通道关了，等着写的任务也该醒来看看。
                s.wake_write();
            }) as Box<dyn FnMut()>)
        };
        let onerror = {
            let shared = shared.clone();
            Closure::wrap(Box::new(move |e: JsValue| {
                let mut s = shared.borrow_mut();
                s.error = Some(format!("{e:?}"));
                s.wake_read();
                s.wake_write();
            }) as Box<dyn FnMut(JsValue)>)
        };

        let onbufferedamountlow = {
            let shared = shared.clone();
            Closure::wrap(Box::new(move || {
                shared.borrow_mut().wake_write();
            }) as Box<dyn FnMut()>)
        };

        dc.set_buffered_amount_low_threshold(LOW_WATER_MARK);
        dc.set_onbufferedamountlow(Some(onbufferedamountlow.as_ref().unchecked_ref()));
        dc.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
        dc.set_onclose(Some(onclose.as_ref().unchecked_ref()));
        dc.set_onerror(Some(onerror.as_ref().unchecked_ref()));

        Self {
            dc,
            shared,
            _callbacks: Rc::new(Callbacks {
                _onmessage: onmessage,
                _onclose: onclose,
                _onerror: onerror,
                _onbufferedamountlow: onbufferedamountlow,
            }),
        }
    }
}

impl AsyncRead for PollDataChannel {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let mut shared = self.shared.borrow_mut();
        if let Some(e) = shared.error.take() {
            return Poll::Ready(Err(io::Error::other(e)));
        }
        if !shared.read_buf.is_empty() {
            let n = buf.len().min(shared.read_buf.len());
            buf[..n].copy_from_slice(&shared.read_buf.split_to(n));
            return Poll::Ready(Ok(n));
        }
        if shared.eof {
            // 0 字节即 EOF，这是 AsyncRead 的约定。
            return Poll::Ready(Ok(0));
        }
        shared.read_waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

impl AsyncWrite for PollDataChannel {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        {
            let mut shared = self.shared.borrow_mut();
            if let Some(e) = shared.error.take() {
                return Poll::Ready(Err(io::Error::other(e)));
            }
            // 背压：缓冲满就等浏览器排空到低水位（onbufferedamountlow 会叫醒我们）。
            if self.dc.buffered_amount() > MAX_BUFFERED_AMOUNT {
                shared.write_waker = Some(cx.waker().clone());
                return Poll::Pending;
            }
        }
        match self.dc.ready_state() {
            RtcDataChannelState::Open => {}
            // 未开或已关都不该再写。浏览器在 closed 上写会抛异常，先挡住。
            state => {
                return Poll::Ready(Err(io::Error::other(format!(
                    "DataChannel 不可写，当前状态 {state:?}"
                ))));
            }
        }
        self.dc
            .send_with_u8_array(buf)
            .map_err(|e| io::Error::other(format!("{e:?}")))?;
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        // 浏览器没有暴露「已送达」的钩子，send 返回即认为已交给底层。
        // 真正的背压由 libp2p framing 之上的应用层负责。
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.dc.close();
        Poll::Ready(Ok(()))
    }
}
