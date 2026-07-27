//! [`ConnectionHandler`] 适配：把 [`Session`] 接到 libp2p 的 poll 模型上。
//!
//! **这里只做 IO 与事件翻译，不含协议决策**——「收到什么该做什么」全在 [`Session`]。
//! 一条 relay 连接配一个 handler。

use std::task::{Context, Poll};

use asynchronous_codec::Framed;
use futures::{SinkExt, StreamExt};
use libp2p_core::muxing::StreamMuxerBox;
use libp2p_core::upgrade::ReadyUpgrade;
use libp2p_swarm::handler::{
    ConnectionEvent, ConnectionHandler, ConnectionHandlerEvent, FullyNegotiatedInbound,
    FullyNegotiatedOutbound, SubstreamProtocol,
};
use libp2p_swarm::{Stream, StreamProtocol};

use crate::backend::Factory;
use crate::config::Config;
use crate::error::Error;
use crate::protocol::{Codec, SIGNALING_PROTOCOL};
use crate::swarm::session::{Action, Role, Session};

/// 本传输的信令协议标识。
pub const PROTOCOL: StreamProtocol = StreamProtocol::new(SIGNALING_PROTOCOL);

/// behaviour → handler。
#[derive(Debug)]
pub enum Command {
    /// 作为发起方开始信令。
    Start,
}

/// handler → behaviour。
#[derive(Debug)]
pub enum Event {
    /// 直连建立（spec 步骤 8），附数据面。
    Connected(StreamMuxerBox),
    /// 信令或建连失败。
    Failed(Error),
}

/// 信令流的 poll 适配器。
#[derive(Debug)]
pub struct Handler {
    session: Session,
    stream: Option<Framed<Stream, Codec>>,
}

impl Handler {
    pub(crate) fn new(config: Config, factory: Factory) -> Self {
        Self {
            session: Session::new(config, factory),
            stream: None,
        }
    }

    /// 待发消息 → 流。返回是否有推进。
    fn flush_outgoing(&mut self, cx: &mut Context<'_>) -> bool {
        let Some(stream) = self.stream.as_mut() else {
            return false;
        };
        let mut progressed = false;
        loop {
            match stream.poll_ready_unpin(cx) {
                Poll::Ready(Ok(())) => {}
                Poll::Ready(Err(e)) => {
                    self.session.fail(Error::Protocol(e));
                    return true;
                }
                Poll::Pending => break,
            }
            let Some(msg) = self.session.next_outgoing() else {
                break;
            };
            if let Err(e) = stream.start_send_unpin(msg) {
                self.session.fail(Error::Protocol(e));
                return true;
            }
            progressed = true;
        }
        if let Some(stream) = self.stream.as_mut()
            && let Poll::Ready(Err(e)) = stream.poll_flush_unpin(cx)
        {
            self.session.fail(Error::Protocol(e));
            return true;
        }
        progressed
    }

    /// 流 → 会话。返回是否有推进。
    fn read_incoming(&mut self, cx: &mut Context<'_>) -> bool {
        let mut progressed = false;
        loop {
            let Some(stream) = self.stream.as_mut() else {
                return progressed;
            };
            match stream.poll_next_unpin(cx) {
                Poll::Ready(Some(Ok(msg))) => {
                    progressed = true;
                    self.session.on_message(msg);
                }
                Poll::Ready(Some(Err(e))) => {
                    self.session.fail(Error::Protocol(e));
                    return true;
                }
                Poll::Ready(None) => {
                    self.session.on_stream_closed();
                    return true;
                }
                Poll::Pending => return progressed,
            }
        }
    }
}

impl ConnectionHandler for Handler {
    type FromBehaviour = Command;
    type ToBehaviour = Event;
    type InboundProtocol = ReadyUpgrade<StreamProtocol>;
    type OutboundProtocol = ReadyUpgrade<StreamProtocol>;
    type InboundOpenInfo = ();
    type OutboundOpenInfo = ();

    fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol, ()> {
        SubstreamProtocol::new(ReadyUpgrade::new(PROTOCOL), ())
    }

    fn connection_keep_alive(&self) -> bool {
        // 信令未完成前必须保住 relay 连接——它断了 SDP 就换不完。
        !self.session.is_finished()
    }

    fn on_behaviour_event(&mut self, event: Self::FromBehaviour) {
        match event {
            Command::Start => self.session.start_as_initiator(),
        }
    }

    fn on_connection_event(
        &mut self,
        event: ConnectionEvent<Self::InboundProtocol, Self::OutboundProtocol>,
    ) {
        match event {
            // 对端开来信令流 —— 应答方路径，无需 behaviour 参与。
            ConnectionEvent::FullyNegotiatedInbound(FullyNegotiatedInbound {
                protocol: stream,
                ..
            }) => {
                self.stream = Some(Framed::new(stream, Codec));
                self.session.attach_stream(Role::Responder);
            }
            // 我们请求的出站流就绪 —— 发起方路径。
            ConnectionEvent::FullyNegotiatedOutbound(FullyNegotiatedOutbound {
                protocol: stream,
                ..
            }) => {
                self.stream = Some(Framed::new(stream, Codec));
                self.session.attach_stream(Role::Initiator);
            }
            ConnectionEvent::DialUpgradeError(err) => {
                self.session
                    .fail(Error::SignalingAborted(format!("开流失败：{}", err.error)));
            }
            ConnectionEvent::ListenUpgradeError(err) => {
                self.session.fail(Error::SignalingAborted(format!(
                    "受理入站流失败：{}",
                    err.error
                )));
            }
            _ => {}
        }
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ConnectionHandlerEvent<Self::OutboundProtocol, (), Self::ToBehaviour>> {
        loop {
            match self.session.poll(cx) {
                Poll::Ready(Action::OpenStream) => {
                    return Poll::Ready(ConnectionHandlerEvent::OutboundSubstreamRequest {
                        protocol: SubstreamProtocol::new(ReadyUpgrade::new(PROTOCOL), ()),
                    });
                }
                Poll::Ready(Action::Connected) => {
                    // spec 步骤 8：成功后关闭信令流。
                    self.stream = None;
                    let event = match self.session.take_muxer() {
                        Some(muxer) => Event::Connected(muxer),
                        // 建连事件到了却拿不到数据面，等于没连上——如实报失败，
                        // 否则上层会拿到一个不能收发的连接。
                        None => Event::Failed(Error::Connection("数据面缺失".into())),
                    };
                    return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(event));
                }
                Poll::Ready(Action::Failed(e)) => {
                    self.stream = None;
                    return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(Event::Failed(e)));
                }
                Poll::Pending => {}
            }

            // 会话无动作时泵 IO；两侧都不动才真正 Pending，避免空转。
            if !(self.flush_outgoing(cx) | self.read_incoming(cx)) {
                return Poll::Pending;
            }
        }
    }
}
