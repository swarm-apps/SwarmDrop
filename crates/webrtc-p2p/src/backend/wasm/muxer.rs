//! 浏览器侧数据面：把 `RTCPeerConnection` 的 DataChannel 暴露为 [`StreamMuxer`]。
//!
//! 结构与 [native 侧](crate::backend::native) 对应，两处必须保持同样的不变量：
//! **跳过 init 通道**、**持续驱动 drop listener**（原因见下方注释）。
//!
//! 浏览器侧简单一些：`createDataChannel` 是同步的，不必像 native 那样存一个进行中的
//! future。整体由 `SendWrapper` 包住以满足 `StreamMuxerBox` 的 `Send` 要求。

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll, Waker, ready};

use futures::channel::mpsc;
use futures::stream::FuturesUnordered;
use futures::{AsyncRead, AsyncWrite, StreamExt};
use libp2p_core::muxing::{StreamMuxer, StreamMuxerEvent};
use send_wrapper::SendWrapper;
use web_sys::{RtcDataChannel, RtcPeerConnection};

use super::callbacks::JsCallbacks;
use super::data_channel::{DEFAULT_MAX_READ_BUFFER, PollDataChannel};
use crate::error::Error;

/// spec 步骤 4 那条只为让 SDP 带上 ICE 信息而建的通道。
///
/// **它不是数据流**：步骤 8 要求建连后关闭它。若把它当子流交给上层，对端会收到一条
/// 永远没有数据的流，libp2p 的协议协商会卡在那儿。
pub(crate) const INIT_CHANNEL_LABEL: &str = "init";

/// direct 模式 Noise 握手所用通道的 SCTP stream id（`negotiated` + id 0）。
///
/// 与 native 侧同名常量含义一致，见那边的说明。
pub(crate) const NOISE_CHANNEL_ID: u16 = 0;

/// libp2p 子流。
///
/// `libp2p_webrtc_utils::Stream<PollDataChannel>` 内部持有 JS 对象（经 `Rc`），不是
/// `Send`，而 `StreamMuxerBox` 的 `SubstreamBox` 要求 `Send`。故与官方 `webrtc-websys`
/// 一样用 `SendWrapper` 包一层并转发读写——wasm 单线程，跨线程访问会 panic，因而安全。
pub(crate) struct Substream {
    inner: SendWrapper<libp2p_webrtc_utils::Stream<PollDataChannel>>,
}

impl Substream {
    /// 把一条 DataChannel 直接包成子流，不登记 drop 通知。
    ///
    /// 只给 direct 模式的 Noise 握手通道用：握手期间没有「子流被取消」这回事，
    /// 握手完成后那条通道也不再使用。业务子流一律走 [`Inner::wrap`]，它会登记
    /// drop 通知——少了那个，子流被 drop 时底层通道不会关闭。
    pub(crate) fn without_drop_listener(
        dc: RtcDataChannel,
        config: libp2p_webrtc_utils::StreamConfig,
    ) -> Self {
        let (stream, drop_listener) = libp2p_webrtc_utils::Stream::with_config(
            PollDataChannel::new(dc, read_buffer_limit(config)),
            config,
        );
        drop(drop_listener);
        Self {
            inner: SendWrapper::new(stream),
        }
    }
}

impl AsyncRead for Substream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut *self.get_mut().inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for Substream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut *self.get_mut().inner).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.get_mut().inner).poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.get_mut().inner).poll_close(cx)
    }
}

/// 一条浏览器 WebRTC 连接的数据面。
pub(crate) struct Muxer {
    inner: SendWrapper<Inner>,
}

struct Inner {
    pc: RtcPeerConnection,
    incoming: mpsc::UnboundedReceiver<RtcDataChannel>,
    next_outbound_id: u64,
    /// 子流的消息尺寸策略。direct 模式沿用 Noise 协商出的值，打洞模式用默认值。
    stream_config: libp2p_webrtc_utils::StreamConfig,
    /// 子流的 drop 通知。必须持续驱动，否则子流被 drop 时底层通道不会关闭，
    /// 连接上会积累永不释放的 DataChannel。
    drop_listeners:
        FuturesUnordered<SendWrapper<libp2p_webrtc_utils::DropListener<PollDataChannel>>>,
    /// `drop_listeners` 空时存下的 waker。
    ///
    /// `FuturesUnordered::poll_next` 返回 `Ready(None)`（集合空）时**不注册 waker**，
    /// 于是之后 `wrap()` 推进去的 listener 要等连接因别的事件被唤醒才会被 poll 到。
    /// 而它没被 poll 完，`PollDataChannel` 的那份 clone 就不释放——**回调迟迟不解绑、
    /// Reset 也迟迟不发**，连接安静下来时尤其明显。故 push 之后主动唤醒。
    no_drop_listeners_waker: Option<Waker>,
    /// 建连期间在 `RTCPeerConnection` 上注册的 JS 回调闭包。
    ///
    /// **必须由连接持有到最后**，两条路径都是：闭包一 drop，JS 侧那个回调就静默失效。
    /// `ondatachannel` 尤其致命——`incoming` 从此永远收不到对端开的子流，连接看着是好的，
    /// 只是再也开不出入站流来。两处踩过的坑各不相同：
    ///
    /// - **direct**：在 `outbound()` 的局部量里注册，函数一返回闭包就没了；
    /// - **打洞**：闭包曾留在 `WasmBackend` 里，而信令会话一完成
    ///   （`connection_keep_alive() == false`）那条 relay 上的信令连接就会被关掉，
    ///   handler → session → backend 顺次 drop，数据面还活着回调却已经死了。
    ///
    /// 收在这里，回调与数据面同寿；drop 时由 [`JsCallbacks`] 负责先解绑再释放。
    _callbacks: JsCallbacks<RtcPeerConnection>,
}

/// 累计读缓冲上限：不得低于单条消息上限，否则一条合法的大消息永远进不来。
///
/// 与单条消息上限**不能混用**：`onmessage` 在 Rust task 再次 poll 之前可能已经攒下多条
/// 各自合法的消息，拿单条上限当累计上限会把正常流量误判成过载。
fn read_buffer_limit(config: libp2p_webrtc_utils::StreamConfig) -> usize {
    DEFAULT_MAX_READ_BUFFER.max(config.max_message_size())
}

impl Muxer {
    /// `stream_config` 是子流的消息尺寸上限：direct 沿用 Noise 握手**协商出来**的值
    /// （两端必须一致，否则合法的大消息会被一侧判成协议违规而重置子流），打洞路径没有
    /// 握手可协商，取默认值。
    ///
    /// `callbacks` 是建连期间注册的 JS 闭包，交给连接续命（见 [`Inner::_callbacks`]）。
    pub(crate) fn new(
        pc: RtcPeerConnection,
        incoming: mpsc::UnboundedReceiver<RtcDataChannel>,
        stream_config: libp2p_webrtc_utils::StreamConfig,
        callbacks: JsCallbacks<RtcPeerConnection>,
    ) -> Self {
        Self {
            inner: SendWrapper::new(Inner {
                pc,
                incoming,
                next_outbound_id: 0,
                stream_config,
                drop_listeners: FuturesUnordered::new(),
                no_drop_listeners_waker: None,
                _callbacks: callbacks,
            }),
        }
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        // 浏览器不会回收一条还在跑 ICE/DTLS 的 `RTCPeerConnection`：Rust 侧 drop 掉句柄
        // 只是撤掉一个引用，连接本身照跑。**数据面是它的最后一个持有者**，由这里负责关。
        //
        // 光靠 `poll_close` 不够——那要 libp2p 走正常关闭流程才会调到，连接异常终止时
        // muxer 是直接被 drop 的，僵尸连接会一条条攒下来。重复 close 无害（W3C 规定幂等）。
        self.pc.close();
    }
}

impl Inner {
    fn wrap(&mut self, dc: RtcDataChannel) -> Substream {
        let (stream, drop_listener) = libp2p_webrtc_utils::Stream::with_config(
            PollDataChannel::new(dc, read_buffer_limit(self.stream_config)),
            self.stream_config,
        );
        self.drop_listeners.push(SendWrapper::new(drop_listener));
        // 集合刚才可能是空的（那时 poll 拿不到 waker），主动把连接叫醒来 poll 新成员。
        if let Some(waker) = self.no_drop_listeners_waker.take() {
            waker.wake();
        }
        Substream {
            inner: SendWrapper::new(stream),
        }
    }
}

impl StreamMuxer for Muxer {
    type Substream = Substream;
    type Error = Error;

    fn poll_inbound(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Self::Substream, Self::Error>> {
        let inner = &mut *self.get_mut().inner;
        loop {
            let Some(dc) = ready!(inner.incoming.poll_next_unpin(cx)) else {
                return Poll::Ready(Err(Error::Connection("连接已关闭".into())));
            };
            // init 通道不是数据流，跳过（见 INIT_CHANNEL_LABEL 的说明）。
            if dc.label() == INIT_CHANNEL_LABEL {
                continue;
            }
            // Noise 握手通道同理（direct 模式）：它由两端各自以 negotiated=true/id=0
            // 建出，按 W3C 规范不该经 `ondatachannel` 上报，但别把这当成保证——
            // native 侧的 rtc 0.20 就照报不误。显式跳过，代价只是一次比较。
            if dc.id() == Some(NOISE_CHANNEL_ID) {
                tracing::debug!("跳过 Noise 握手通道（stream id 0），它不是业务子流");
                continue;
            }
            return Poll::Ready(Ok(inner.wrap(dc)));
        }
    }

    fn poll_outbound(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Result<Self::Substream, Self::Error>> {
        let inner = &mut *self.get_mut().inner;
        // 浏览器的 createDataChannel 是同步的，直接拿到即可。
        let label = format!("libp2p-{}", inner.next_outbound_id);
        inner.next_outbound_id += 1;
        let dc = inner.pc.create_data_channel(&label);
        Poll::Ready(Ok(inner.wrap(dc)))
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.get_mut().inner.pc.close();
        Poll::Ready(Ok(()))
    }

    fn poll(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<StreamMuxerEvent, Self::Error>> {
        let inner = &mut *self.get_mut().inner;
        // 驱动 drop 通知：子流被 drop 时靠它关闭底层 DataChannel。
        loop {
            match inner.drop_listeners.poll_next_unpin(cx) {
                Poll::Ready(Some(_)) => continue,
                // 集合空了，`FuturesUnordered` 不会替我们注册 waker，自己存下。
                Poll::Ready(None) => {
                    inner.no_drop_listeners_waker = Some(cx.waker().clone());
                    break;
                }
                Poll::Pending => break,
            }
        }
        Poll::Pending
    }
}
