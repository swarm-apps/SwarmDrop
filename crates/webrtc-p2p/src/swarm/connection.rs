//! 建立完成的 WebRTC 连接。
//!
//! 它是 [`Transport::Output`](libp2p_core::Transport::Output) 的载荷：既带对端身份，
//! 也**就是**数据面——直接实现 [`StreamMuxer`]，委托给后端交出的 [`StreamMuxerBox`]。
//!
//! 接入 swarm 时按 libp2p 惯例再包一层：
//! `.map(|(peer, conn), _| (peer, StreamMuxerBox::new(conn)))`。

use std::pin::Pin;
use std::task::{Context, Poll};

use libp2p_core::muxing::{StreamMuxer, StreamMuxerEvent};
use libp2p_core::muxing::{StreamMuxerBox, SubstreamBox};
use libp2p_identity::PeerId;

/// 一条建立完成的 WebRTC 连接。
#[derive(Debug)]
pub struct Connection {
    peer: PeerId,
    muxer: StreamMuxerBox,
}

impl Connection {
    pub(crate) fn new(peer: PeerId, muxer: StreamMuxerBox) -> Self {
        Self { peer, muxer }
    }

    /// 对端身份。
    ///
    /// 由信令所经的**已认证** relay 连接确定，而非对端自报——SDP 里的 DTLS 指纹在
    /// 握手时被验证，两者共同完成身份绑定（见 crate 文档「为什么不需要额外的 Noise」）。
    pub fn peer(&self) -> PeerId {
        self.peer
    }
}

impl StreamMuxer for Connection {
    type Substream = SubstreamBox;
    type Error = std::io::Error;

    fn poll_inbound(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Self::Substream, Self::Error>> {
        Pin::new(&mut self.get_mut().muxer).poll_inbound(cx)
    }

    fn poll_outbound(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Self::Substream, Self::Error>> {
        Pin::new(&mut self.get_mut().muxer).poll_outbound(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.get_mut().muxer).poll_close(cx)
    }

    fn poll(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<StreamMuxerEvent, Self::Error>> {
        Pin::new(&mut self.get_mut().muxer).poll(cx)
    }
}
