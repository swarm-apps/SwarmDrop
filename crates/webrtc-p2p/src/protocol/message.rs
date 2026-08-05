//! Wire format of `/webrtc-signaling/0.0.1`.
//!
//! As defined by the spec (the Signaling Protocol section of `webrtc/webrtc.md`):
//! messages are protobuf-encoded and prefixed with their byte length as an
//! unsigned-varint.
//!
//! ```protobuf
//! message Message {
//!     enum Type {
//!         SDP_OFFER = 0;
//!         SDP_ANSWER = 1;
//!         ICE_CANDIDATE = 2;
//!     }
//!     optional Type type = 1;
//!     optional string data = 2;
//! }
//! ```
//!
//! # Why the codec is hand-written instead of generated
//!
//! Two fields and two wire types come to fewer than 100 hand-written lines, in exchange
//! for zero codegen steps and a smaller wasm binary. The cost is having to uphold two
//! proto3 semantics by hand (see below), which is why the tests pin the encoding
//! byte-for-byte with **golden bytes** — interoperating with js-libp2p requires an
//! identical wire format, and a single wrong byte here means the handshake never
//! succeeds.

use std::fmt;

/// The protocol ID mandated by the spec.
pub const SIGNALING_PROTOCOL: &str = "/webrtc-signaling/0.0.1";

/// Upper bound on the length of a single signaling message.
///
/// Not specified by the spec. An SDP is usually a few KB and an ICE candidate a few
/// hundred bytes; 64 KiB leaves ample headroom while preventing a malicious peer from
/// dragging us into a huge allocation with an oversized varint header.
pub const MAX_MESSAGE_LEN: usize = 64 * 1024;

/// protobuf field tags (defined by the spec; immutable — change one and js-libp2p no
/// longer recognizes us).
///
/// tag = `(field number << 3) | wire_type`. The on-the-wire bytes are written out
/// literally here so they can be compared against the golden tests.
mod field {
    /// `optional Type type = 1;` → `(1 << 3) | 0` (varint)
    pub const TYPE_TAG: u8 = 0x08;
    /// `optional string data = 2;` → `(2 << 3) | 2` (length-delimited)
    pub const DATA_TAG: u8 = 0x12;
}

/// Message type. The discriminants are fixed by the spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
    /// The `RTCSessionDescription.sdp` string.
    SdpOffer = 0,
    /// The `RTCSessionDescription.sdp` string.
    SdpAnswer = 1,
    /// The JSON string from `RTCIceCandidate.toJSON()`.
    IceCandidate = 2,
}

impl MessageType {
    fn from_raw(v: u64) -> Result<Self, Error> {
        match v {
            0 => Ok(Self::SdpOffer),
            1 => Ok(Self::SdpAnswer),
            2 => Ok(Self::IceCandidate),
            other => Err(Error::UnknownMessageType(other)),
        }
    }
}

/// A single signaling message.
///
/// Both fields are proto3 `optional` (they have presence), so `Option` represents them
/// faithfully — "unset" and "set to the default value" are two different on-the-wire
/// encodings and must not be collapsed into one.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Message {
    pub ty: Option<MessageType>,
    pub data: Option<String>,
}

impl Message {
    /// SDP offer (spec step 4).
    pub fn offer(sdp: impl Into<String>) -> Self {
        Self {
            ty: Some(MessageType::SdpOffer),
            data: Some(sdp.into()),
        }
    }

    /// SDP answer (spec step 5).
    pub fn answer(sdp: impl Into<String>) -> Self {
        Self {
            ty: Some(MessageType::SdpAnswer),
            data: Some(sdp.into()),
        }
    }

    /// Trickle ICE candidate (spec step 7). `json` is the result of
    /// `RTCIceCandidate.toJSON()`.
    pub fn ice_candidate(json: impl Into<String>) -> Self {
        Self {
            ty: Some(MessageType::IceCandidate),
            data: Some(json.into()),
        }
    }

    /// Encodes to protobuf (without the length prefix).
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        // ⚠️ proto3 presence 语义：`optional` 字段一旦 set 就必须写入，**即使值等于
        // 默认值**。SDP_OFFER 恰好是 0，若沿用「零值省略」的普通 proto3 习惯，offer
        // 消息发出去对端只会看到 type=None，整条链路从第一步就断。
        if let Some(ty) = self.ty {
            out.push(field::TYPE_TAG);
            let mut buf = unsigned_varint::encode::u64_buffer();
            out.extend_from_slice(unsigned_varint::encode::u64(ty as u64, &mut buf));
        }
        if let Some(data) = &self.data {
            out.push(field::DATA_TAG);
            let mut buf = unsigned_varint::encode::usize_buffer();
            out.extend_from_slice(unsigned_varint::encode::usize(data.len(), &mut buf));
            out.extend_from_slice(data.as_bytes());
        }
        out
    }

    /// Decodes from protobuf (without the length prefix).
    ///
    /// Unknown fields are skipped according to their wire type rather than rejected — when
    /// the spec adds fields later, older implementations can still read the parts they
    /// understand.
    pub fn decode(mut bytes: &[u8]) -> Result<Self, Error> {
        let mut msg = Self::default();
        while !bytes.is_empty() {
            let tag = bytes[0];
            bytes = &bytes[1..];
            match tag {
                field::TYPE_TAG => {
                    let (v, rest) = unsigned_varint::decode::u64(bytes)?;
                    msg.ty = Some(MessageType::from_raw(v)?);
                    bytes = rest;
                }
                field::DATA_TAG => {
                    let (len, rest) = unsigned_varint::decode::usize(bytes)?;
                    if len > rest.len() {
                        return Err(Error::Truncated);
                    }
                    let (s, rest) = rest.split_at(len);
                    msg.data = Some(String::from_utf8(s.to_vec()).map_err(|_| Error::InvalidUtf8)?);
                    bytes = rest;
                }
                other => bytes = skip_unknown_field(other, bytes)?,
            }
        }
        Ok(msg)
    }

    /// Encodes a complete frame with the unsigned-varint length prefix (the spec's
    /// on-the-wire shape).
    pub fn encode_framed(&self) -> Vec<u8> {
        let body = self.encode();
        let mut buf = unsigned_varint::encode::usize_buffer();
        let prefix = unsigned_varint::encode::usize(body.len(), &mut buf);
        let mut out = Vec::with_capacity(prefix.len() + body.len());
        out.extend_from_slice(prefix);
        out.extend_from_slice(&body);
        out
    }

    /// Decodes one frame from the front of a byte stream, returning the message and how
    /// many bytes the frame consumed.
    ///
    /// Returns [`Error::Incomplete`] when there is not enough data — the caller should read
    /// more and retry, and **must not** treat it as a protocol error and reset the
    /// stream.
    pub fn decode_framed(input: &[u8]) -> Result<(Self, usize), Error> {
        let (len, rest) = match unsigned_varint::decode::usize(input) {
            Ok(v) => v,
            // varint 头本身还没读全
            Err(unsigned_varint::decode::Error::Insufficient) => return Err(Error::Incomplete),
            Err(e) => return Err(e.into()),
        };
        if len > MAX_MESSAGE_LEN {
            return Err(Error::TooLong(len));
        }
        if rest.len() < len {
            return Err(Error::Incomplete);
        }
        let consumed = input.len() - rest.len() + len;
        Ok((Self::decode(&rest[..len])?, consumed))
    }
}

/// Skips an unknown field and returns the remaining bytes.
fn skip_unknown_field(tag: u8, bytes: &[u8]) -> Result<&[u8], Error> {
    match tag & 0x07 {
        // varint
        0 => Ok(unsigned_varint::decode::u64(bytes)?.1),
        // 64-bit
        1 => bytes.get(8..).ok_or(Error::Truncated),
        // length-delimited
        2 => {
            let (len, rest) = unsigned_varint::decode::usize(bytes)?;
            rest.get(len..).ok_or(Error::Truncated)
        }
        // 32-bit
        5 => bytes.get(4..).ok_or(Error::Truncated),
        wire => Err(Error::UnknownWireType(wire)),
    }
}

/// Errors from encoding or decoding a signaling message.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("frame is incomplete")]
    Incomplete,
    #[error("field is truncated")]
    Truncated,
    #[error("message length {0} exceeds the limit of {MAX_MESSAGE_LEN}")]
    TooLong(usize),
    #[error("the `data` field is not valid UTF-8")]
    InvalidUtf8,
    #[error("unknown message type discriminant: {0}")]
    UnknownMessageType(u64),
    #[error("unknown protobuf wire type: {0}")]
    UnknownWireType(u8),
    #[error("varint decoding failed: {0}")]
    Varint(#[from] unsigned_varint::decode::Error),
    /// An I/O error on the signaling stream itself.
    ///
    /// `asynchronous-codec` requires a codec's error type to be convertible from
    /// `io::Error` (it reports underlying read/write errors through the same channel), so
    /// this variant has to exist.
    #[error("signaling stream I/O failed: {0}")]
    Io(#[from] std::io::Error),
}

impl fmt::Display for MessageType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::SdpOffer => "SDP_OFFER",
            Self::SdpAnswer => "SDP_ANSWER",
            Self::IceCandidate => "ICE_CANDIDATE",
        };
        f.write_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 黄金字节：与 js-libp2p 互通的唯一保证是 wire format 逐字节一致，
    /// 故这里钉死人工推导的编码，而不是只做 round-trip（round-trip 对「双向都错」免疫）。
    #[test]
    fn golden_encoding_offer() {
        // field 1 (tag 0x08) = 0（SDP_OFFER）；field 2 (tag 0x12) len 3 = "v=0"
        let expect = [0x08, 0x00, 0x12, 0x03, b'v', b'=', b'0'];
        assert_eq!(Message::offer("v=0").encode(), expect);
    }

    #[test]
    fn golden_encoding_answer_and_candidate() {
        assert_eq!(
            Message::answer("a").encode(),
            [0x08, 0x01, 0x12, 0x01, b'a']
        );
        assert_eq!(
            Message::ice_candidate("c").encode(),
            [0x08, 0x02, 0x12, 0x01, b'c']
        );
    }

    /// 最关键的一条：SDP_OFFER 判别值是 0，普通 proto3 的「零值省略」会让它消失。
    #[test]
    fn zero_valued_type_is_still_encoded() {
        let bytes = Message::offer("x").encode();
        assert_eq!(bytes[0], field::TYPE_TAG, "type 字段必须出现");
        assert_eq!(bytes[1], 0x00, "值为 0 也要写入");
        assert_eq!(
            Message::decode(&bytes).unwrap().ty,
            Some(MessageType::SdpOffer)
        );
    }

    #[test]
    fn unset_fields_are_omitted() {
        assert!(Message::default().encode().is_empty());
        let only_data = Message {
            ty: None,
            data: Some("d".into()),
        };
        assert_eq!(only_data.encode(), [0x12, 0x01, b'd']);
        assert_eq!(Message::decode(&only_data.encode()).unwrap(), only_data);
    }

    #[test]
    fn roundtrip_all_types() {
        for msg in [
            Message::offer("v=0\r\no=- 0 0 IN IP4 0.0.0.0\r\n"),
            Message::answer("v=0\r\n"),
            Message::ice_candidate(r#"{"candidate":"candidate:1 1 udp ...","sdpMid":"0"}"#),
        ] {
            assert_eq!(Message::decode(&msg.encode()).unwrap(), msg);
        }
    }

    #[test]
    fn framed_roundtrip_and_consumed_len() {
        let msg = Message::offer("v=0");
        let framed = msg.encode_framed();
        assert_eq!(
            framed[0] as usize,
            framed.len() - 1,
            "长度前缀应等于负载长度"
        );

        let (decoded, consumed) = Message::decode_framed(&framed).unwrap();
        assert_eq!(decoded, msg);
        assert_eq!(consumed, framed.len());
    }

    /// 粘包：一次读到多帧时要能逐帧推进。
    #[test]
    fn framed_decodes_stream_of_messages() {
        let mut buf = Vec::new();
        let msgs = [
            Message::offer("a"),
            Message::answer("b"),
            Message::ice_candidate("c"),
        ];
        for m in &msgs {
            buf.extend_from_slice(&m.encode_framed());
        }
        let mut rest = &buf[..];
        for expect in &msgs {
            let (got, n) = Message::decode_framed(rest).unwrap();
            assert_eq!(&got, expect);
            rest = &rest[n..];
        }
        assert!(rest.is_empty());
    }

    /// 半包必须是可重试的 Incomplete，不能被当成协议错误去 reset 流。
    #[test]
    fn partial_frame_reports_incomplete() {
        let framed = Message::offer("v=0").encode_framed();
        for cut in 1..framed.len() {
            assert!(
                matches!(
                    Message::decode_framed(&framed[..cut]),
                    Err(Error::Incomplete)
                ),
                "截断到 {cut} 字节时应为 Incomplete"
            );
        }
        assert!(matches!(
            Message::decode_framed(&[]),
            Err(Error::Incomplete)
        ));
    }

    #[test]
    fn oversized_frame_is_rejected_before_allocating() {
        let mut buf = unsigned_varint::encode::usize_buffer();
        let prefix = unsigned_varint::encode::usize(MAX_MESSAGE_LEN + 1, &mut buf).to_vec();
        assert!(matches!(
            Message::decode_framed(&prefix),
            Err(Error::TooLong(_))
        ));
    }

    /// 向前兼容：spec 将来加字段，旧实现要能读懂自己认识的部分。
    #[test]
    fn unknown_fields_are_skipped() {
        let mut bytes = Message::offer("v=0").encode();
        bytes.extend_from_slice(&[0x18, 0x7f]); // field 3, varint
        bytes.extend_from_slice(&[0x22, 0x02, 0xde, 0xad]); // field 4, length-delimited
        bytes.extend_from_slice(&[0x2d, 0, 0, 0, 0]); // field 5, 32-bit
        bytes.extend_from_slice(&[0x31, 0, 0, 0, 0, 0, 0, 0, 0]); // field 6, 64-bit
        assert_eq!(Message::decode(&bytes).unwrap(), Message::offer("v=0"));
    }

    #[test]
    fn invalid_inputs_are_errors_not_panics() {
        assert!(matches!(
            Message::decode(&[0x08, 0x63]),
            Err(Error::UnknownMessageType(99))
        ));
        // data 字段声称 5 字节但只有 1 字节
        assert!(matches!(
            Message::decode(&[0x12, 0x05, b'x']),
            Err(Error::Truncated)
        ));
        // 非法 UTF-8
        assert!(matches!(
            Message::decode(&[0x12, 0x01, 0xff]),
            Err(Error::InvalidUtf8)
        ));
        // wire type 3/4（已废弃的 group）
        assert!(matches!(
            Message::decode(&[(7 << 3) | 3]),
            Err(Error::UnknownWireType(3))
        ));
    }
}
