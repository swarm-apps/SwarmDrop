//! 信令流的 framed 编解码。
//!
//! 把 [`signaling`](crate::signaling) 的字节级编解码接到 `asynchronous-codec` 上，
//! 使信令流可以按 [`Message`] 收发而不必自己维护缓冲状态机。

use asynchronous_codec::{Decoder, Encoder};
use bytes::BytesMut;

use crate::signaling::{self, Message};

/// `/webrtc-signaling/0.0.1` 的 codec。
#[derive(Debug, Default, Clone, Copy)]
pub struct Codec;

impl Encoder for Codec {
    type Item<'a> = Message;
    type Error = signaling::Error;

    fn encode(&mut self, item: Self::Item<'_>, dst: &mut BytesMut) -> Result<(), Self::Error> {
        dst.extend_from_slice(&item.encode_framed());
        Ok(())
    }
}

impl Decoder for Codec {
    type Item = Message;
    type Error = signaling::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        match Message::decode_framed(src) {
            Ok((msg, consumed)) => {
                // 只吃掉本帧，剩余字节留给下一次 decode——粘包时一次 read 可能带来多帧。
                let _ = src.split_to(consumed);
                Ok(Some(msg))
            }
            // 半包不是错误：`asynchronous-codec` 约定返回 Ok(None) 以等待更多字节。
            // 若当成 Err 返回，一次不完整的读就会把整条信令流 reset 掉。
            Err(signaling::Error::Incomplete) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signaling::MessageType;

    #[test]
    fn encode_then_decode() {
        let mut codec = Codec;
        let mut buf = BytesMut::new();
        codec.encode(Message::offer("v=0"), &mut buf).unwrap();

        let got = codec.decode(&mut buf).unwrap().expect("应解出一帧");
        assert_eq!(got, Message::offer("v=0"));
        assert!(buf.is_empty(), "解完应把本帧字节吃干净");
    }

    /// 粘包：一次 read 带来多帧时要能连续解出，且不吃掉后续帧。
    #[test]
    fn decodes_multiple_frames_from_one_buffer() {
        let mut codec = Codec;
        let mut buf = BytesMut::new();
        let msgs = [
            Message::offer("a"),
            Message::answer("b"),
            Message::ice_candidate("c"),
        ];
        for m in &msgs {
            codec.encode(m.clone(), &mut buf).unwrap();
        }
        for expect in &msgs {
            assert_eq!(codec.decode(&mut buf).unwrap().as_ref(), Some(expect));
        }
        assert!(codec.decode(&mut buf).unwrap().is_none());
    }

    /// 半包必须是 Ok(None)（等更多字节），不能是 Err——否则一次不完整的读就 reset 流。
    #[test]
    fn partial_frame_yields_none_not_error() {
        let mut codec = Codec;
        let full = Message::offer("v=0").encode_framed();
        for cut in 1..full.len() {
            let mut buf = BytesMut::from(&full[..cut]);
            assert!(
                matches!(codec.decode(&mut buf), Ok(None)),
                "截断到 {cut} 字节应为 Ok(None)"
            );
            assert_eq!(buf.len(), cut, "半包时不应消耗任何字节");
        }
    }

    #[test]
    fn accumulating_partial_then_completing() {
        let mut codec = Codec;
        let full = Message::ice_candidate("{\"candidate\":\"x\"}").encode_framed();
        let mut buf = BytesMut::from(&full[..2]);
        assert!(codec.decode(&mut buf).unwrap().is_none());

        buf.extend_from_slice(&full[2..]);
        let got = codec.decode(&mut buf).unwrap().expect("补齐后应解出");
        assert_eq!(got.ty, Some(MessageType::IceCandidate));
    }
}
