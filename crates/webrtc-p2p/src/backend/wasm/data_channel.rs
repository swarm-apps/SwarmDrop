//! 浏览器 `RTCDataChannel` → `AsyncRead + AsyncWrite` 适配。
//!
//! 与 [native 侧](crate::backend::native) 同名模块解决同一个问题，但形状不同：浏览器侧
//! 是**回调驱动**（`onmessage` / `onclose`），不是可 await 的事件流。故用闭包把消息推进
//! 队列，poll 时从队列取。
//!
//! # 三条约束
//!
//! 1. **闭包必须被持有，且释放前必须解绑**。两条一起由
//!    [`JsCallbacks`](super::callbacks::JsCallbacks) 承担，原因见那里的模块文档。
//! 2. **唤醒必须延后到回调栈之外**，见 [`defer_wake`]。
//! 3. **必须 `Clone`**（`Stream<T>` 的要求），且 clone 出的副本要共享同一条通道与同一份
//!    队列，因此状态全部收在 `Rc<RefCell<_>>` 里。
//!
//! `SendWrapper` 由上层（[`super::WasmBackend`]）统一施加：本类型自身不跨线程流转，
//! 是被 `Stream<T>` 持有后随连接一起被包起来的。

use std::cell::RefCell;
use std::io;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use bytes::{Buf, BytesMut};
use futures::{AsyncRead, AsyncWrite};
use wasm_bindgen::prelude::*;
use web_sys::{MessageEvent, RtcDataChannel, RtcDataChannelState, RtcDataChannelType};

use super::callbacks::JsCallbacks;

/// 发送缓冲上限。超过就背压，等浏览器排空到低水位再继续。
///
/// 没有它，快生产者能把浏览器的发送缓冲无限撑大——spike 实测过同类失败：8 MiB 突发
/// 会把链路打满，连接直接断掉而非降速。native 侧靠「等上一条 send 完成」实现背压，
/// 浏览器侧只能靠 bufferedAmount 自己数。
const MAX_BUFFERED_AMOUNT: u32 = 1024 * 1024;

/// 低水位。降到这里才唤醒写侧——留出一半余量，避免在阈值附近反复抖动。
const LOW_WATER_MARK: u32 = MAX_BUFFERED_AMOUNT / 2;

/// 累计读缓冲的默认上限（本地资源上限，不参与协商）。
///
/// **不能取官方 `webrtc-websys` 的 256 KiB。** 那个值比本端自己的发送高水位
/// [`MAX_BUFFERED_AMOUNT`]（1 MiB）还小——对端可以合法地让 1 MiB 数据在途，接收侧的
/// Rust task 只要停顿一下（浏览器侧是 OPFS 落盘 + bao 逐块校验，一个应用块 256 KiB），
/// 堆积就会越过 256 KiB，于是**正常的快速传输被判成「对端过载」并重置子流**。
/// 取 4 MiB：高于在途上限一个数量级的余量，又远低于会把标签页撑爆的量级。
///
/// 实际上限由调用方给（见 `muxer::read_buffer_limit`）——本类型只是字节流包装，
/// 不认识 libp2p 的 framing 配置。
pub(crate) const DEFAULT_MAX_READ_BUFFER: usize = 4 * 1024 * 1024;

/// 在**当前 JS 回调栈之外**唤醒任务。
///
/// 直接在回调里 `wake()` 会同步重入 executor，而此刻 wasm-bindgen 仍持有该闭包的可变
/// 借用（`Closure<dyn FnMut(_)>`）：重入进同一个回调就抛
/// `closure invoked recursively or after being dropped`，重入进 poll 路径则会撞上
/// `Shared` 的第二次 `borrow_mut` 直接 panic。官方 `webrtc-websys` 的同一条修复见
/// libp2p [#6558](https://github.com/libp2p/rust-libp2p/pull/6558)。
///
/// 收 `Option` 是为了让每个回调都保持「取出 waker → 释放借用 → 唤醒」这一直线形状。
fn defer_wake(waker: Option<Waker>) {
    let Some(waker) = waker else {
        return;
    };
    wasm_bindgen_futures::spawn_local(async move {
        waker.wake();
    });
}

/// 把浏览器 `RTCDataChannel` 包装成字节流。
///
/// `Clone` 语义是「共享同一条通道」，不是复制。
#[derive(Clone)]
pub(crate) struct PollDataChannel {
    dc: RtcDataChannel,
    shared: Rc<RefCell<Shared>>,
    /// 回调闭包。放这里是为了让它们与「最后一个副本」同寿——drop 之后回调就该解绑了。
    _callbacks: Rc<JsCallbacks<RtcDataChannel>>,
}

#[derive(Default)]
struct Shared {
    /// 已到达但尚未被读走的字节。
    read_buf: BytesMut,
    /// 对端已关闭，读侧到达 EOF。
    eof: bool,
    /// 通道出错（`onerror`）。**一个字节都没丢**，缓冲里是完整合法的前缀。
    errored: Option<String>,
    /// 读缓冲越限：有消息被**丢弃**了，缓冲从此是一段有洞的字节流。
    overloaded: bool,
    /// 等待可读的任务。
    read_waker: Option<Waker>,
    /// 因发送缓冲满而等待的任务。
    write_waker: Option<Waker>,
}

impl Shared {
    /// 两种终态错误**都不清除**：不可恢复，且读写两侧都要能看到——取走即清会让先醒来的
    /// 那一侧独吞。
    ///
    /// 但它们**不能合并成一个字段**，因为相对读缓冲的顺序恰好相反（见 `poll_read`）：
    /// 越限丢过消息，缓冲有洞、必须连同缓冲一起作废；`onerror` 没丢数据，缓冲得先排干。
    /// 合并过一次，症状是发送方一关连接（SCTP ABORT → 浏览器 `error` 事件），接收侧就把
    /// 已收到但没读完的尾部连同 FIN flag 一起丢掉，正常收尾被报成流被 reset。
    fn overloaded(&self) -> Option<io::Error> {
        self.overloaded
            .then(|| io::Error::new(io::ErrorKind::BrokenPipe, "对端发送速度超出本端读取速度"))
    }

    fn errored(&self) -> Option<io::Error> {
        self.errored.as_deref().map(io::Error::other)
    }
}

/// 注册一个只唤醒写侧的回调。
///
/// `onopen`（SCTP 开了）与 `onbufferedamountlow`（排空到低水位）对写侧是同一件事：
/// 「现在可以再写了」。
fn wake_writer(shared: &Rc<RefCell<Shared>>) -> Closure<dyn FnMut()> {
    let shared = shared.clone();
    Closure::wrap(Box::new(move || {
        // 借用到此为止，唤醒发生在它之后——理由见 `defer_wake`。
        let waker = shared.borrow_mut().write_waker.take();
        defer_wake(waker);
    }) as Box<dyn FnMut()>)
}

/// 置终态并把读写两侧一起叫醒。
fn finish(shared: &RefCell<Shared>, set: impl FnOnce(&mut Shared)) {
    let (read, write) = {
        let mut s = shared.borrow_mut();
        set(&mut s);
        (s.read_waker.take(), s.write_waker.take())
    };
    defer_wake(read);
    defer_wake(write);
}

impl PollDataChannel {
    /// `max_read_buffer` 是累计读缓冲上限，由调用方按连接协商出的消息尺寸算
    /// （见 `muxer::read_buffer_limit`）。
    pub(crate) fn new(dc: RtcDataChannel, max_read_buffer: usize) -> Self {
        // 必须收 ArrayBuffer：默认是 Blob，取字节要再走一次异步读取，凭空多一层状态。
        dc.set_binary_type(RtcDataChannelType::Arraybuffer);

        let shared = Rc::new(RefCell::new(Shared::default()));

        let onmessage = {
            let shared = shared.clone();
            Closure::wrap(Box::new(move |ev: MessageEvent| {
                let data = js_sys::Uint8Array::new(&ev.data());
                let incoming = data.length() as usize;
                let waker = {
                    let mut s = shared.borrow_mut();
                    let buffered = s.read_buf.len();
                    if buffered.saturating_add(incoming) > max_read_buffer {
                        // 读缓冲无上限地涨下去就是浏览器 OOM。置错让上层重置这条子流，
                        // 这是唯一能施加的背压——SCTP 侧我们够不着。
                        tracing::warn!(
                            buffered,
                            incoming,
                            max_read_buffer,
                            "对端消息堆积超过读缓冲上限，重置子流"
                        );
                        s.overloaded = true;
                    } else {
                        // 直接拷进目标缓冲。`to_vec()` 会多一次堆分配和一次全量拷贝
                        // ——16 KiB 的消息上限下那是这条回调里最贵的一笔。
                        s.read_buf.resize(buffered + incoming, 0);
                        data.copy_to(&mut s.read_buf[buffered..]);
                    }
                    s.read_waker.take()
                };
                defer_wake(waker);
            }) as Box<dyn FnMut(MessageEvent)>)
        };
        // 通道关了，等着写的任务也该醒来看看，故与 onerror 同走 `finish`。
        let onclose = {
            let shared = shared.clone();
            Closure::wrap(Box::new(move || finish(&shared, |s| s.eof = true)) as Box<dyn FnMut()>)
        };
        let onerror = {
            let shared = shared.clone();
            Closure::wrap(Box::new(move |e: JsValue| {
                finish(&shared, |s| {
                    s.errored.get_or_insert_with(|| format!("{e:?}"));
                });
            }) as Box<dyn FnMut(JsValue)>)
        };
        // onopen 是 Connecting → Open 的唯一通知。少了它，在 SCTP 握手完成前发起的写会
        // 永远挂着——没有任何其他事件会叫醒它（onmessage 只在收到数据时来）。
        let onopen = wake_writer(&shared);
        let onbufferedamountlow = wake_writer(&shared);

        dc.set_buffered_amount_low_threshold(LOW_WATER_MARK);
        dc.set_onbufferedamountlow(Some(onbufferedamountlow.as_ref().unchecked_ref()));
        dc.set_onopen(Some(onopen.as_ref().unchecked_ref()));
        dc.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
        dc.set_onclose(Some(onclose.as_ref().unchecked_ref()));
        dc.set_onerror(Some(onerror.as_ref().unchecked_ref()));

        Self {
            dc: dc.clone(),
            shared,
            _callbacks: Rc::new(JsCallbacks::new(
                dc,
                |dc: &RtcDataChannel| {
                    dc.set_onopen(None);
                    dc.set_onmessage(None);
                    dc.set_onclose(None);
                    dc.set_onerror(None);
                    dc.set_onbufferedamountlow(None);
                },
                (onopen, onmessage, onclose, onerror, onbufferedamountlow),
            )),
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
        // 顺序是语义的一部分，别调：越限**先于**缓冲（缓冲已有洞，接着解帧只会解出垃圾），
        // eof 与 onerror **后于**缓冲（没丢数据，先让调用方把已到达的读完）。
        if let Some(e) = shared.overloaded() {
            return Poll::Ready(Err(e));
        }
        if !shared.read_buf.is_empty() {
            let n = buf.len().min(shared.read_buf.len());
            buf[..n].copy_from_slice(&shared.read_buf[..n]);
            // `advance` 而非 `split_to`：后者会把缓冲提升成共享态（`KIND_ARC`），
            // 此后每次读都多一对引用计数，且回收头部空间要走 memmove 慢路径。
            shared.read_buf.advance(n);
            return Poll::Ready(Ok(n));
        }
        if shared.eof {
            // 0 字节即 EOF，这是 AsyncRead 的约定。
            return Poll::Ready(Ok(0));
        }
        if let Some(e) = shared.errored() {
            return Poll::Ready(Err(e));
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
            // 写侧不必区分两者：读缓冲怎么排干与写无关，出错就是不能再写。
            if let Some(e) = shared.overloaded().or_else(|| shared.errored()) {
                return Poll::Ready(Err(e));
            }
            // 背压：缓冲满就等浏览器排空到低水位（onbufferedamountlow 会叫醒我们）。
            if self.dc.buffered_amount() > MAX_BUFFERED_AMOUNT {
                shared.write_waker = Some(cx.waker().clone());
                return Poll::Pending;
            }
        }
        match self.dc.ready_state() {
            RtcDataChannelState::Open => {}
            // **SCTP 握手未完成，不是错误。** PeerConnection 的 `connected`（DTLS 完成）
            // 早于 DataChannel 的 `open`（SCTP 完成），muxer 一交出去上层就可能开始写，
            // 此刻必然还是 Connecting。曾把它当错误返回，症状是打洞连接刚建立就
            // 「DataChannel 不可写，当前状态 Connecting」刷屏——实际只差几十毫秒。
            RtcDataChannelState::Connecting => {
                self.shared.borrow_mut().write_waker = Some(cx.waker().clone());
                return Poll::Pending;
            }
            // 已关就不该再写。浏览器在 closed 上写会抛异常，先挡住。
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
