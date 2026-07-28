//! 浏览器侧数据面：把 `RTCPeerConnection` 的 DataChannel 暴露为 [`StreamMuxer`]。
//!
//! 结构与 [native 侧](crate::backend::native) 对应，两处必须保持同样的不变量：
//! **跳过 init 通道**、**持续驱动 drop listener**（原因见下方注释）。
//!
//! 浏览器侧简单一些：`createDataChannel` 是同步的，不必像 native 那样存一个进行中的
//! future。整体由 `SendWrapper` 包住以满足 `StreamMuxerBox` 的 `Send` 要求。

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll, ready};

use futures::channel::mpsc;
use futures::stream::FuturesUnordered;
use futures::{AsyncRead, AsyncWrite, StreamExt};
use libp2p_core::muxing::{StreamMuxer, StreamMuxerEvent};
use send_wrapper::SendWrapper;
use web_sys::{RtcDataChannel, RtcPeerConnection};

use super::data_channel::PollDataChannel;
use crate::error::Error;

/// spec 步骤 4 那条只为让 SDP 带上 ICE 信息而建的通道。
///
/// **它不是数据流**：步骤 8 要求建连后关闭它。若把它当子流交给上层，对端会收到一条
/// 永远没有数据的流，libp2p 的协议协商会卡在那儿。
pub(crate) const INIT_CHANNEL_LABEL: &str = "init";

/// libp2p 子流。
///
/// `libp2p_webrtc_utils::Stream<PollDataChannel>` 内部持有 JS 对象（经 `Rc`），不是
/// `Send`，而 `StreamMuxerBox` 的 `SubstreamBox` 要求 `Send`。故与官方 `webrtc-websys`
/// 一样用 `SendWrapper` 包一层并转发读写——wasm 单线程，跨线程访问会 panic，因而安全。
pub(crate) struct Substream {
    inner: SendWrapper<libp2p_webrtc_utils::Stream<PollDataChannel>>,
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
    /// 子流的 drop 通知。必须持续驱动，否则子流被 drop 时底层通道不会关闭，
    /// 连接上会积累永不释放的 DataChannel。
    drop_listeners:
        FuturesUnordered<SendWrapper<libp2p_webrtc_utils::DropListener<PollDataChannel>>>,
}

impl Muxer {
    pub(crate) fn new(
        pc: RtcPeerConnection,
        incoming: mpsc::UnboundedReceiver<RtcDataChannel>,
    ) -> Self {
        Self {
            inner: SendWrapper::new(Inner {
                pc,
                incoming,
                next_outbound_id: 0,
                drop_listeners: FuturesUnordered::new(),
            }),
        }
    }
}

impl Inner {
    fn wrap(&mut self, dc: RtcDataChannel) -> Substream {
        let (stream, drop_listener) = libp2p_webrtc_utils::Stream::new(PollDataChannel::new(dc));
        self.drop_listeners.push(SendWrapper::new(drop_listener));
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
        while let Poll::Ready(Some(_)) = inner.drop_listeners.poll_next_unpin(cx) {}
        Poll::Pending
    }
}
