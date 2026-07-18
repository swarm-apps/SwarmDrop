//! transfer-data 数据通道帧协议。
//!
//! 这里是 SwarmDrop 应用层协议：新内核只负责打开裸流（[`P2pStream`](swarmdrop_net::P2pStream)），
//! 本模块负责帧边界、版本、session/epoch 绑定和传输语义。协议名见
//! [`protocol::TRANSFER_DATA_PROTOCOL`](crate::protocol::TRANSFER_DATA_PROTOCOL)。

use std::io;

use futures::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use uuid::Uuid;

use crate::protocol::{FileInfo, FileRange};
use crate::{AppError, AppResult};

/// Hello 帧中的协议版本（wire v2）。
pub const TRANSFER_DATA_VERSION: u16 = 2;

/// 单帧最大 payload。256KiB 明文块 + 帧头，8MiB 给协议扩展和测试留余量。
pub const MAX_FRAME_LEN: usize = 8 * 1024 * 1024;

// TAG 3（旧逐块 Ack）与 4（旧 BlockRequest 重传）已废弃；编号留空洞不复用，避免与历史帧混淆。
const TAG_HELLO: u8 = 1;
const TAG_BLOCK_DATA: u8 = 2;
const TAG_ABORT: u8 = 5;
const TAG_FINISH: u8 = 6;

/// 数据通道握手角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferDataRole {
    Sender,
    Receiver,
}

impl TransferDataRole {
    fn as_u8(self) -> u8 {
        match self {
            Self::Sender => 1,
            Self::Receiver => 2,
        }
    }

    fn from_u8(value: u8) -> AppResult<Self> {
        match value {
            1 => Ok(Self::Sender),
            2 => Ok(Self::Receiver),
            _ => Err(protocol_error(format!("未知 transfer-data role: {value}"))),
        }
    }
}

/// 数据通道帧。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferDataFrame {
    Hello {
        session_id: Uuid,
        epoch: i64,
        role: TransferDataRole,
        manifest_digest: [u8; 32],
        fetch_plan: Vec<FileRange>,
    },
    BlockData {
        session_id: Uuid,
        epoch: i64,
        range: FileRange,
        /// 明文块数据（wire v2 已删应用层加密，见 [`wire`](crate::transfer::wire)）。
        data: Vec<u8>,
        /// 逐块完整性证明的扩展位（bao-tree 接入预留，见知识库
        /// iroh-migration.md 的选型结论）。v2 当前恒为 `None`；接入后携带
        /// 该 chunk group 的 outboard 证明，接收端**在文件收完前**即可逐块
        /// 验证——取代「续传信任对端」的现状。字段进 v2 布局定义（u8 标志 +
        /// 可选 len-prefixed bytes），接入时无需 bump 协议版本。
        proof: Option<Vec<u8>>,
    },
    Abort {
        session_id: Uuid,
        epoch: i64,
        reason: String,
    },
    Finish {
        session_id: Uuid,
        epoch: i64,
    },
}

impl TransferDataFrame {
    pub fn session_id(&self) -> Uuid {
        match self {
            Self::Hello { session_id, .. }
            | Self::BlockData { session_id, .. }
            | Self::Abort { session_id, .. }
            | Self::Finish { session_id, .. } => *session_id,
        }
    }

    pub fn epoch(&self) -> i64 {
        match self {
            Self::Hello { epoch, .. }
            | Self::BlockData { epoch, .. }
            | Self::Abort { epoch, .. }
            | Self::Finish { epoch, .. } => *epoch,
        }
    }
}

/// 对 manifest 做稳定 digest，用于 Hello 握手校验双方看到的是同一批文件。
pub fn manifest_digest(files: &[FileInfo]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    for file in files {
        hasher.update(&file.file_id.to_be_bytes());
        put_string_to_hasher(&mut hasher, &file.name);
        put_string_to_hasher(&mut hasher, &file.relative_path);
        hasher.update(&file.size.to_be_bytes());
        put_string_to_hasher(&mut hasher, &file.checksum);
    }
    *hasher.finalize().as_bytes()
}

/// 首次传输的 fetch plan：按文件顺序发送完整内容。
pub fn full_fetch_plan(files: &[FileInfo]) -> Vec<FileRange> {
    files
        .iter()
        .map(|file| FileRange {
            file_id: file.file_id,
            offset: 0,
            length: file.size,
        })
        .collect()
}

fn put_string_to_hasher(hasher: &mut blake3::Hasher, value: &str) {
    hasher.update(&(value.len() as u64).to_be_bytes());
    hasher.update(value.as_bytes());
}

/// 写入一个 length-prefixed frame。
pub async fn write_frame<W>(writer: &mut W, frame: &TransferDataFrame) -> AppResult<()>
where
    W: AsyncWrite + Unpin,
{
    let payload = encode_frame(frame)?;
    if payload.len() > MAX_FRAME_LEN {
        return Err(protocol_error(format!(
            "transfer-data frame 超出长度限制: {} > {}",
            payload.len(),
            MAX_FRAME_LEN
        )));
    }

    let mut len_buf = unsigned_varint::encode::usize_buffer();
    let len = unsigned_varint::encode::usize(payload.len(), &mut len_buf);
    writer.write_all(len).await.map_err(io_error)?;
    writer.write_all(&payload).await.map_err(io_error)?;
    writer.flush().await.map_err(io_error)?;
    Ok(())
}

/// 读取一个 length-prefixed frame。读到干净 EOF 时返回 `Ok(None)`。
pub async fn read_frame<R>(reader: &mut R) -> AppResult<Option<TransferDataFrame>>
where
    R: AsyncRead + Unpin,
{
    let len = match unsigned_varint::aio::read_usize(&mut *reader).await {
        Ok(len) => len,
        Err(unsigned_varint::io::ReadError::Io(e)) if e.kind() == io::ErrorKind::UnexpectedEof => {
            return Ok(None);
        }
        Err(e) => return Err(protocol_error(format!("读取 frame 长度失败: {e}"))),
    };
    if len > MAX_FRAME_LEN {
        return Err(protocol_error(format!(
            "transfer-data frame 超出长度限制: {len} > {MAX_FRAME_LEN}"
        )));
    }
    let mut payload = vec![0u8; len];
    reader.read_exact(&mut payload).await.map_err(io_error)?;
    decode_frame(&payload).map(Some)
}

fn encode_frame(frame: &TransferDataFrame) -> AppResult<Vec<u8>> {
    let mut buf = Vec::new();
    match frame {
        TransferDataFrame::Hello {
            session_id,
            epoch,
            role,
            manifest_digest,
            fetch_plan,
        } => {
            push_context(&mut buf, TAG_HELLO, *session_id, *epoch);
            push_u16(&mut buf, TRANSFER_DATA_VERSION);
            buf.push(role.as_u8());
            buf.extend_from_slice(manifest_digest);
            push_ranges(&mut buf, fetch_plan)?;
        }
        TransferDataFrame::BlockData {
            session_id,
            epoch,
            range,
            data,
            proof,
        } => {
            push_context(&mut buf, TAG_BLOCK_DATA, *session_id, *epoch);
            push_range(&mut buf, range);
            push_bytes(&mut buf, data)?;
            // 逐块证明扩展位：u8 标志 + 可选 len-prefixed bytes
            match proof {
                Some(proof) => {
                    buf.push(1);
                    push_bytes(&mut buf, proof)?;
                }
                None => buf.push(0),
            }
        }
        TransferDataFrame::Abort {
            session_id,
            epoch,
            reason,
        } => {
            push_context(&mut buf, TAG_ABORT, *session_id, *epoch);
            push_string(&mut buf, reason)?;
        }
        TransferDataFrame::Finish { session_id, epoch } => {
            push_context(&mut buf, TAG_FINISH, *session_id, *epoch);
        }
    }
    Ok(buf)
}

fn decode_frame(payload: &[u8]) -> AppResult<TransferDataFrame> {
    let mut cursor = Cursor::new(payload);
    let tag = cursor.take_u8()?;
    let session_id = Uuid::from_bytes(cursor.take_array()?);
    let epoch = cursor.take_i64()?;
    let frame = match tag {
        TAG_HELLO => {
            let version = cursor.take_u16()?;
            if version != TRANSFER_DATA_VERSION {
                return Err(protocol_error(format!(
                    "不支持的 transfer-data version: {version}"
                )));
            }
            let role = TransferDataRole::from_u8(cursor.take_u8()?)?;
            let manifest_digest = cursor.take_array()?;
            let fetch_plan = cursor.take_ranges()?;
            TransferDataFrame::Hello {
                session_id,
                epoch,
                role,
                manifest_digest,
                fetch_plan,
            }
        }
        TAG_BLOCK_DATA => {
            let range = cursor.take_range()?;
            let data = cursor.take_bytes()?;
            let proof = match cursor.take_u8()? {
                0 => None,
                1 => Some(cursor.take_bytes()?),
                other => {
                    return Err(protocol_error(format!(
                        "非法的 BlockData proof 标志: {other}"
                    )));
                }
            };
            TransferDataFrame::BlockData {
                session_id,
                epoch,
                range,
                data,
                proof,
            }
        }
        TAG_ABORT => TransferDataFrame::Abort {
            session_id,
            epoch,
            reason: cursor.take_string()?,
        },
        TAG_FINISH => TransferDataFrame::Finish { session_id, epoch },
        _ => {
            return Err(protocol_error(format!(
                "未知 transfer-data frame tag: {tag}"
            )));
        }
    };
    cursor.finish()?;
    Ok(frame)
}

fn push_context(buf: &mut Vec<u8>, tag: u8, session_id: Uuid, epoch: i64) {
    buf.push(tag);
    buf.extend_from_slice(session_id.as_bytes());
    buf.extend_from_slice(&epoch.to_be_bytes());
}

fn push_ranges(buf: &mut Vec<u8>, ranges: &[FileRange]) -> AppResult<()> {
    let count = u32::try_from(ranges.len())
        .map_err(|_| protocol_error("fetch_plan 过长，无法编码为 u32".into()))?;
    push_u32(buf, count);
    for range in ranges {
        push_range(buf, range);
    }
    Ok(())
}

fn push_range(buf: &mut Vec<u8>, range: &FileRange) {
    push_u32(buf, range.file_id);
    push_u64(buf, range.offset);
    push_u64(buf, range.length);
}

fn push_string(buf: &mut Vec<u8>, value: &str) -> AppResult<()> {
    push_bytes(buf, value.as_bytes())
}

fn push_bytes(buf: &mut Vec<u8>, value: &[u8]) -> AppResult<()> {
    let len = u32::try_from(value.len())
        .map_err(|_| protocol_error("字段过长，无法编码为 u32".into()))?;
    push_u32(buf, len);
    buf.extend_from_slice(value);
    Ok(())
}

fn push_u16(buf: &mut Vec<u8>, value: u16) {
    buf.extend_from_slice(&value.to_be_bytes());
}

fn push_u32(buf: &mut Vec<u8>, value: u32) {
    buf.extend_from_slice(&value.to_be_bytes());
}

fn push_u64(buf: &mut Vec<u8>, value: u64) {
    buf.extend_from_slice(&value.to_be_bytes());
}

struct Cursor<'a> {
    payload: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(payload: &'a [u8]) -> Self {
        Self { payload, pos: 0 }
    }

    fn finish(&self) -> AppResult<()> {
        if self.pos == self.payload.len() {
            Ok(())
        } else {
            Err(protocol_error(format!(
                "frame payload 尾部有多余字节: {}",
                self.payload.len() - self.pos
            )))
        }
    }

    fn take_u8(&mut self) -> AppResult<u8> {
        let bytes = self.take(1)?;
        Ok(bytes[0])
    }

    fn take_u16(&mut self) -> AppResult<u16> {
        Ok(u16::from_be_bytes(self.take_array()?))
    }

    fn take_u32(&mut self) -> AppResult<u32> {
        Ok(u32::from_be_bytes(self.take_array()?))
    }

    fn take_i64(&mut self) -> AppResult<i64> {
        Ok(i64::from_be_bytes(self.take_array()?))
    }

    fn take_u64(&mut self) -> AppResult<u64> {
        Ok(u64::from_be_bytes(self.take_array()?))
    }

    fn take_array<const N: usize>(&mut self) -> AppResult<[u8; N]> {
        let bytes = self.take(N)?;
        let mut out = [0u8; N];
        out.copy_from_slice(bytes);
        Ok(out)
    }

    fn take_range(&mut self) -> AppResult<FileRange> {
        Ok(FileRange {
            file_id: self.take_u32()?,
            offset: self.take_u64()?,
            length: self.take_u64()?,
        })
    }

    fn take_ranges(&mut self) -> AppResult<Vec<FileRange>> {
        let count = self.take_u32()? as usize;
        let mut ranges = Vec::with_capacity(count);
        for _ in 0..count {
            ranges.push(self.take_range()?);
        }
        Ok(ranges)
    }

    fn take_string(&mut self) -> AppResult<String> {
        let bytes = self.take_bytes()?;
        String::from_utf8(bytes).map_err(|e| protocol_error(format!("frame 字符串不是 UTF-8: {e}")))
    }

    fn take_bytes(&mut self) -> AppResult<Vec<u8>> {
        let len = self.take_u32()? as usize;
        Ok(self.take(len)?.to_vec())
    }

    fn take(&mut self, len: usize) -> AppResult<&'a [u8]> {
        let end = self
            .pos
            .checked_add(len)
            .ok_or_else(|| protocol_error("frame payload 长度溢出".into()))?;
        if end > self.payload.len() {
            return Err(protocol_error("frame payload 截断".into()));
        }
        let slice = &self.payload[self.pos..end];
        self.pos = end;
        Ok(slice)
    }
}

fn io_error(err: io::Error) -> AppError {
    AppError::Transfer(format!("transfer-data IO 错误: {err}"))
}

fn protocol_error(message: String) -> AppError {
    AppError::Transfer(format!("transfer-data 协议错误: {message}"))
}

#[cfg(test)]
mod tests {
    use futures::io::Cursor as IoCursor;

    use super::*;

    fn session_id() -> Uuid {
        Uuid::from_u128(0x11112222333344445555666677778888)
    }

    fn range() -> FileRange {
        FileRange {
            file_id: 7,
            offset: 1024,
            length: 4096,
        }
    }

    #[tokio::test]
    async fn frame_roundtrip_preserves_context_and_payload() {
        let frame = TransferDataFrame::BlockData {
            session_id: session_id(),
            epoch: 3,
            range: range(),
            data: vec![1, 2, 3, 4, 5],
            proof: None,
        };

        let mut io = IoCursor::new(Vec::new());
        write_frame(&mut io, &frame).await.unwrap();
        io.set_position(0);

        let decoded = read_frame(&mut io).await.unwrap().unwrap();
        assert_eq!(decoded, frame);

        // 逐块证明扩展位（bao-tree 预留）：Some 分支同样 roundtrip
        let with_proof = TransferDataFrame::BlockData {
            session_id: session_id(),
            epoch: 3,
            range: range(),
            data: vec![1, 2, 3],
            proof: Some(vec![0xAA; 64]),
        };
        let mut io = IoCursor::new(Vec::new());
        write_frame(&mut io, &with_proof).await.unwrap();
        io.set_position(0);
        assert_eq!(read_frame(&mut io).await.unwrap().unwrap(), with_proof);
        assert_eq!(decoded.session_id(), session_id());
        assert_eq!(decoded.epoch(), 3);
    }

    #[tokio::test]
    async fn hello_roundtrip_preserves_fetch_plan_and_digest() {
        let files = vec![FileInfo {
            file_id: 1,
            name: "a.txt".into(),
            relative_path: "dir/a.txt".into(),
            size: 42,
            checksum: "checksum".into(),
        }];
        let frame = TransferDataFrame::Hello {
            session_id: session_id(),
            epoch: 5,
            role: TransferDataRole::Sender,
            manifest_digest: manifest_digest(&files),
            fetch_plan: vec![range()],
        };

        let mut io = IoCursor::new(Vec::new());
        write_frame(&mut io, &frame).await.unwrap();
        io.set_position(0);

        assert_eq!(read_frame(&mut io).await.unwrap().unwrap(), frame);
    }

    #[tokio::test]
    async fn oversized_frame_is_rejected_before_allocation() {
        let mut prefix = unsigned_varint::encode::usize_buffer();
        let encoded_len = unsigned_varint::encode::usize(MAX_FRAME_LEN + 1, &mut prefix);
        let mut io = IoCursor::new(encoded_len.to_vec());

        let err = read_frame(&mut io).await.unwrap_err();
        assert!(err.to_string().contains("超出长度限制"));
    }

    #[test]
    fn unknown_role_is_rejected() {
        assert!(TransferDataRole::from_u8(99).is_err());
    }
}
