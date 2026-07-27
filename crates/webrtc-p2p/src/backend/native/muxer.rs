//! 数据面：把 `PeerConnection` 的 DataChannel 暴露为 libp2p 的 [`StreamMuxer`]。
//!
//! 每条 libp2p 子流对应一条 DataChannel，字节流由 [`PollDataChannel`] 适配、framing 由
//! `libp2p_webrtc_utils::Stream` 承担。

use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, ready};

use futures::channel::mpsc;
use futures::future::BoxFuture;
use futures::stream::FuturesUnordered;
use futures::{FutureExt, StreamExt};
use libp2p_core::muxing::{StreamMuxer, StreamMuxerEvent};
use webrtc::data_channel::DataChannel;
use webrtc::peer_connection::PeerConnection;

use super::data_channel::PollDataChannel;
use crate::error::Error;

/// spec 步骤 4 那条只为让 SDP 带上 ICE 信息而建的通道。
///
/// **它不是数据流**：步骤 8 要求建连后关闭它。若把它当子流交给上层，对端会收到一条
/// 永远没有数据的流，libp2p 的协议协商会卡在那儿。
pub(crate) const INIT_CHANNEL_LABEL: &str = "init";

/// 一条 WebRTC 连接的数据面。
pub(crate) struct Muxer {
    pc: Arc<dyn PeerConnection>,
    /// 对端开来的 DataChannel（由后端的事件回调投递）。
    incoming: mpsc::UnboundedReceiver<Arc<dyn DataChannel>>,
    /// 正在创建的出站 DataChannel。
    creating: Option<BoxFuture<'static, Result<Arc<dyn DataChannel>, Error>>>,
    /// 出站通道的自增编号，仅用于生成互不重复的 label。
    next_outbound_id: u64,
    /// 子流的 drop 通知。必须持续驱动，否则子流被 drop 时底层通道不会关闭，
    /// 连接上会积累永不释放的 DataChannel。
    drop_listeners: FuturesUnordered<libp2p_webrtc_utils::DropListener<PollDataChannel>>,
}

impl Muxer {
    pub(crate) fn new(
        pc: Arc<dyn PeerConnection>,
        incoming: mpsc::UnboundedReceiver<Arc<dyn DataChannel>>,
    ) -> Self {
        Self {
            pc,
            incoming,
            creating: None,
            next_outbound_id: 0,
            drop_listeners: FuturesUnordered::new(),
        }
    }

    /// 把一条 DataChannel 包成 libp2p 子流，并登记它的 drop 通知。
    fn wrap(&mut self, dc: Arc<dyn DataChannel>) -> libp2p_webrtc_utils::Stream<PollDataChannel> {
        let (stream, drop_listener) = libp2p_webrtc_utils::Stream::new(PollDataChannel::new(dc));
        self.drop_listeners.push(drop_listener);
        stream
    }
}

impl StreamMuxer for Muxer {
    type Substream = libp2p_webrtc_utils::Stream<PollDataChannel>;
    type Error = Error;

    fn poll_inbound(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Self::Substream, Self::Error>> {
        let this = self.get_mut();
        loop {
            let Some(dc) = ready!(this.incoming.poll_next_unpin(cx)) else {
                return Poll::Ready(Err(Error::Connection("连接已关闭".into())));
            };
            // init 通道不是数据流，跳过（见 INIT_CHANNEL_LABEL 的说明）。
            if dc.label().now_or_never().and_then(Result::ok).as_deref() == Some(INIT_CHANNEL_LABEL)
            {
                continue;
            }
            return Poll::Ready(Ok(this.wrap(dc)));
        }
    }

    fn poll_outbound(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Self::Substream, Self::Error>> {
        let this = self.get_mut();
        if this.creating.is_none() {
            let pc = this.pc.clone();
            let label = format!("libp2p-{}", this.next_outbound_id);
            this.next_outbound_id += 1;
            this.creating = Some(
                async move {
                    pc.create_data_channel(&label, None)
                        .await
                        .map_err(|e| Error::Connection(format!("创建 DataChannel 失败：{e}")))
                }
                .boxed(),
            );
        }
        let fut = this.creating.as_mut().expect("刚置入");
        let dc = ready!(fut.poll_unpin(cx));
        this.creating = None;
        Poll::Ready(dc.map(|dc| this.wrap(dc)))
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // `PeerConnection::close` 是 async；这里不持有它的 future，交由 Drop 时释放。
        // libp2p 关闭连接后不会再 poll 本 muxer，故无需等待完成。
        Poll::Ready(Ok(()))
    }

    fn poll(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<StreamMuxerEvent, Self::Error>> {
        let this = self.get_mut();
        // 驱动 drop 通知：子流被 drop 时靠它关闭底层 DataChannel。
        // 返回值无需处理——完成即达成目的，出错也只说明通道已经没了。
        while let Poll::Ready(Some(_)) = this.drop_listeners.poll_next_unpin(cx) {}
        Poll::Pending
    }
}
