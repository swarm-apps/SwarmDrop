//! An established WebRTC connection.
//!
//! It is the payload of [`Transport::Output`](libp2p_core::Transport::Output): it carries
//! the remote's identity and **is** the data plane — it implements [`StreamMuxer`]
//! directly, delegating to the [`StreamMuxerBox`] handed over by the backend.
//!
//! Following libp2p convention, it gets one more wrapper when plugged into the swarm:
//! `.map(|(peer, conn), _| (peer, StreamMuxerBox::new(conn)))`。

use std::pin::Pin;
use std::task::{Context, Poll};

use libp2p_core::muxing::{StreamMuxer, StreamMuxerEvent};
use libp2p_core::muxing::{StreamMuxerBox, SubstreamBox};
use libp2p_identity::PeerId;

/// An established WebRTC connection.
#[derive(Debug)]
pub struct Connection {
    peer: PeerId,
    muxer: StreamMuxerBox,
}

impl Connection {
    pub(crate) fn new(peer: PeerId, muxer: StreamMuxerBox) -> Self {
        Self { peer, muxer }
    }

    /// The remote's identity.
    ///
    /// Determined by the **authenticated** relay connection the signaling travelled over,
    /// not self-reported by the remote — the DTLS fingerprint in the SDP is verified during
    /// the handshake, and together the two bind the identity (see the crate docs on why no
    /// extra Noise handshake is needed).
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
