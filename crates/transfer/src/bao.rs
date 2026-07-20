//! bao-tree 逐块完整性验证。
//!
//! 补上「文件收完前每个块可验证」的能力（取代「续传信任对端」）。用 bao-tree 的
//! **库级 encode/decode 路径**——不手写 Merkle 验证（易错）。
//!
//! ## 选型：proof 携带完整 bao 切片，`BlockData.data` 置空
//!
//! 每个 [`BlockData`](crate::wire::data_frame::TransferDataFrame) 的 `proof` 字段直接放
//! [`encode_ranges_validated`] 产出的完整 bao 切片（size header + 交错的 Parent/Leaf），
//! `data` 字段置空。接收端把整段喂 [`decode_ranges`]（root = `FileInfo.checksum` 解析回
//! blake3::Hash）——decode 必然验签、无 skip 选项，验过即得明文块写盘。
//!
//! 为何不拆 Parent 进 proof、Leaf 进 data：库没有稳定的「拆/组交错流」公开迭代顺序 API，
//! 手动交错易错；而完整切片方案里 data 置空，叶子只在 proof 出现一次，**不产生 2x 冗余**
//! （wire 开销 ≈ 明文 + parents ≈ 0.4%）。两方案都不改 wire 布局（proof 是 opaque bytes）。
//!
//! ## root 零成本
//!
//! `BlockSize::from_chunk_log(4)`（16KiB chunk group）下 bao 树根 == 标准 blake3 整文件
//! hash（chunk group 只影响 outboard 深度，不影响 root）。`FileInfo.checksum` 已是
//! prepare 流式算出的标准 blake3 hex，直接当验证 root，FileInfo 不加字段。

use std::io::Cursor;
use std::sync::Arc;

use bao_tree::io::fsm::CreateOutboard;
use bao_tree::io::outboard::{PostOrderOutboard, PreOrderOutboard};
use bao_tree::io::round_up_to_chunks;
use bao_tree::io::sync::{
    ReadAt, WriteAt, decode_ranges, encode_ranges_validated, outboard_post_order,
};
use bao_tree::{BaoTree, BlockSize, ByteRanges};
use bytes::Bytes;
use iroh_io::AsyncSliceReader;

use crate::host::{FileAccess, FileSourceId};
use crate::{AppError, AppResult};

/// 16KiB chunk group（iroh `IROH_BLOCK_SIZE` 同款）。CHUNK_SIZE 256KiB 是其整数倍，
/// fetch_plan 天然对齐；文件尾部非对齐块由 bao 依 `file_size` 自行处理。
pub const BLOCK_SIZE: BlockSize = BlockSize::from_chunk_log(4);

/// 从**完整文件字节**构建 post-order outboard，返回 `(root, outboard_bytes)`。
///
/// `root` == 标准 `blake3(file)` == `FileInfo.checksum`（见模块文档）。发送端在 prepare
/// 阶段建一次、随 PreparedFile 持有；resume 时从持久化端口载入（缺失则本函数重算）。
pub fn build_outboard(data: &[u8]) -> (blake3::Hash, Vec<u8>) {
    let tree = BaoTree::new(data.len() as u64, BLOCK_SIZE);
    let mut outboard = Vec::new();
    // 内存 Cursor 读永不失败。
    let root = outboard_post_order(&mut Cursor::new(data), tree, &mut outboard)
        .expect("in-memory outboard build never fails");
    (root, outboard)
}

/// 从 host 的 [`FileAccess`] **流式**构建 post-order outboard（内存有界，不整文件入内存）。
///
/// 经 iroh-io 的 [`AsyncSliceReader`] 适配 async 分块读——避免「async FileAccess ↔ sync
/// outboard 构建」的桥接。返回 `(root, outboard_bytes)`，`root` == 标准 blake3 ==
/// `FileInfo.checksum`。resume 时若持久化的 outboard 缺失，走此路重算并回存。
///
/// 与 [`build_outboard`]（sync in-memory，供单测/小数据）产出**同序**（post-order），
/// 故 [`encode_proof`] 用同一个 [`PostOrderOutboard`] 重建即可，无论哪条路构建。
pub async fn build_outboard_from_source(
    file_access: &Arc<dyn FileAccess>,
    source_id: &FileSourceId,
    size: u64,
) -> AppResult<(blake3::Hash, Vec<u8>)> {
    let reader = FileAccessReader {
        file_access: file_access.clone(),
        source_id: source_id.clone(),
        size,
    };
    let ob = PostOrderOutboard::<Vec<u8>>::create(reader, BLOCK_SIZE)
        .await
        .map_err(|e| AppError::Transfer(format!("bao outboard 构建失败: {e}")))?;
    Ok((ob.root, ob.data))
}

/// [`AsyncSliceReader`] 适配层：把 bao outboard 构建的 async 随机读映射到 [`FileAccess`]。
struct FileAccessReader {
    file_access: Arc<dyn FileAccess>,
    source_id: FileSourceId,
    size: u64,
}

impl AsyncSliceReader for FileAccessReader {
    async fn read_at(&mut self, offset: u64, len: usize) -> std::io::Result<Bytes> {
        let chunk = self
            .file_access
            .read_source_chunk(&self.source_id, offset, len)
            .await
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        // 契约防御：宿主超长返回会把非法长度喂进 bao 的 subtree hasher（blake3 直接
        // panic，2026-07 桌面宿主取整 offset 的事故形态）。响错优于截断——超长通常
        // 伴随 offset 错位，截断会静默产出错误 hash。
        if chunk.len() > len {
            return Err(std::io::Error::other(format!(
                "read_source_chunk 违反契约: 请求 {len}B@{offset}，返回 {}B",
                chunk.len()
            )));
        }
        Ok(Bytes::from(chunk))
    }

    async fn size(&mut self) -> std::io::Result<u64> {
        Ok(self.size)
    }
}

/// 发送端：为 `[offset, offset+block.len())` 生成 bao 证明切片。
///
/// `block` 是该 range 的明文；`outboard_bytes`/`root`/`file_size` 描述整棵树。返回的切片
/// 自带 size header + 交错 Parent/Leaf，接收端 [`decode_and_verify`] 独立可验。
pub fn encode_proof(
    outboard_bytes: &[u8],
    root: blake3::Hash,
    file_size: u64,
    offset: u64,
    block: &[u8],
) -> AppResult<Vec<u8>> {
    // 0 字节文件唯一的空块：无叶子可验，bao 的 range 迭代器不接受空 ranges。返回空
    // proof（仍 Some，保持「None = 协议违规」不变量）；文件之空由 checksum==blake3("") 在
    // 清单层保证。
    if block.is_empty() {
        return Ok(Vec::new());
    }
    let tree = BaoTree::new(file_size, BLOCK_SIZE);
    let outboard = PostOrderOutboard {
        root,
        tree,
        data: outboard_bytes,
    };
    let end = offset + block.len() as u64;
    let ranges = round_up_to_chunks(&ByteRanges::from(offset..end));
    let reader = OffsetReadAt {
        base: offset,
        data: block,
    };
    let mut proof = Vec::new();
    encode_ranges_validated(reader, outboard, &ranges, &mut proof)
        .map_err(|e| AppError::Transfer(format!("bao encode 失败: {e}")))?;
    Ok(proof)
}

/// 接收端：解码并**验证** bao 证明切片，返回验证过的明文块。
///
/// `root` 由 `FileInfo.checksum` 解析（[`root_from_checksum`]）。验证失败 / proof 损坏 →
/// `Err`（调用方按协议违规断流走 Interrupted 恢复）。
pub fn decode_and_verify(
    proof: &[u8],
    root: blake3::Hash,
    file_size: u64,
    offset: u64,
    expected_len: u64,
) -> AppResult<Vec<u8>> {
    // 对称特判：0 长度块无叶子可验，空 proof → 空数据（见 encode_proof）。
    if expected_len == 0 {
        return Ok(Vec::new());
    }
    let tree = BaoTree::new(file_size, BLOCK_SIZE);
    let end = offset + expected_len;
    let ranges = round_up_to_chunks(&ByteRanges::from(offset..end));
    // 接收端不建 outboard（不做再分发）：throwaway outboard 只承载 root 供验签，decode 写进去的
    // parents 用完即弃。data: Vec<u8> 同时是 WriteAt（承载 parents）。
    let mut outboard = PreOrderOutboard {
        root,
        tree,
        data: Vec::<u8>::new(),
    };
    let mut target = OffsetWriteAt {
        base: offset,
        data: vec![0u8; expected_len as usize],
    };
    decode_ranges(Cursor::new(proof), &ranges, &mut target, &mut outboard)
        .map_err(|e| AppError::Transfer(format!("bao 逐块验证失败: {e}")))?;
    Ok(target.data)
}

/// 把 `FileInfo.checksum`（blake3 hex）解析回验证 root。
pub fn root_from_checksum(checksum: &str) -> AppResult<blake3::Hash> {
    blake3::Hash::from_hex(checksum)
        .map_err(|e| AppError::Transfer(format!("checksum 不是合法 blake3 hex: {e}")))
}

/// 把绝对文件偏移 rebase 到块内偏移的 [`ReadAt`]（encode 只读 `ranges` 内，故 `pos >= base`）。
struct OffsetReadAt<'a> {
    base: u64,
    data: &'a [u8],
}

impl ReadAt for OffsetReadAt<'_> {
    fn read_at(&self, pos: u64, buf: &mut [u8]) -> std::io::Result<usize> {
        let rel = pos.checked_sub(self.base).ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "read_at 越过块起点")
        })? as usize;
        let available = self.data.len().saturating_sub(rel);
        let n = available.min(buf.len());
        buf[..n].copy_from_slice(&self.data[rel..rel + n]);
        Ok(n)
    }
}

/// 把绝对文件偏移 rebase 到块内偏移的 [`WriteAt`]，`data` 收 decode 出的验证过明文。
struct OffsetWriteAt {
    base: u64,
    data: Vec<u8>,
}

impl WriteAt for OffsetWriteAt {
    fn write_at(&mut self, pos: u64, buf: &[u8]) -> std::io::Result<usize> {
        let rel = pos.checked_sub(self.base).ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "write_at 越过块起点")
        })? as usize;
        let end = rel + buf.len();
        if self.data.len() < end {
            self.data.resize(end, 0);
        }
        self.data[rel..end].copy_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CHUNK_SIZE;
    use crate::host::{FileSinkId, FinalizedSink, HostFileMetadata};

    /// 造 `size` 字节的确定性伪随机数据（每字节都不同，便于定位篡改）。
    fn data_of(size: usize) -> Vec<u8> {
        (0..size).map(|i| (i * 31 + 7) as u8).collect()
    }

    /// 最小 FileAccess：只服务 read_source_chunk（供 outboard 流式构建测试）。
    /// `ignore_len: true` 复刻 2026-07 桌面宿主违约形态——无视 len 返回 offset 到
    /// 文件尾的全部字节。
    struct MockSource {
        data: Vec<u8>,
        ignore_len: bool,
    }

    #[async_trait::async_trait]
    impl FileAccess for MockSource {
        async fn source_metadata(&self, _s: &FileSourceId) -> AppResult<HostFileMetadata> {
            unreachable!()
        }
        async fn read_source_chunk(
            &self,
            _s: &FileSourceId,
            offset: u64,
            len: usize,
        ) -> AppResult<Vec<u8>> {
            let start = offset as usize;
            let end = if self.ignore_len {
                self.data.len()
            } else {
                (start + len).min(self.data.len())
            };
            Ok(self.data.get(start..end).unwrap_or_default().to_vec())
        }
        async fn create_sink(&self, _m: HostFileMetadata) -> AppResult<FileSinkId> {
            unreachable!()
        }
        async fn write_sink_chunk(&self, _s: &FileSinkId, _o: u64, _d: Vec<u8>) -> AppResult<()> {
            unreachable!()
        }
        async fn finalize_sink(&self, _s: &FileSinkId) -> AppResult<FinalizedSink> {
            unreachable!()
        }
    }

    #[tokio::test]
    async fn streaming_build_matches_in_memory_and_flat_blake3() {
        let data = data_of(CHUNK_SIZE * 2 + 77 * 1024);
        let (mem_root, mem_ob) = build_outboard(&data);
        let source: Arc<dyn FileAccess> = Arc::new(MockSource {
            data: data.clone(),
            ignore_len: false,
        });
        let (stream_root, stream_ob) =
            build_outboard_from_source(&source, &FileSourceId("x".into()), data.len() as u64)
                .await
                .unwrap();
        assert_eq!(
            stream_root,
            blake3::hash(&data),
            "流式 root 必须等于扁平 blake3"
        );
        assert_eq!(stream_root, mem_root, "流式与内存构建 root 一致");
        assert_eq!(
            stream_ob, mem_ob,
            "流式与内存构建 outboard 字节一致（同序）"
        );
    }

    /// 按 256KiB 逐块 encode→decode，断言每块 roundtrip 一致。
    fn roundtrip_all_blocks(data: &[u8]) {
        let (root, outboard) = build_outboard(data);
        // 设计前提：bao 树根 == 标准 blake3 整文件 hash。
        assert_eq!(root, blake3::hash(data), "bao root 必须等于扁平 blake3");

        let size = data.len() as u64;
        let mut offset = 0u64;
        while offset < size || (size == 0 && offset == 0) {
            let len = ((size - offset) as usize).min(CHUNK_SIZE);
            let block = &data[offset as usize..offset as usize + len];
            let proof = encode_proof(&outboard, root, size, offset, block).unwrap();
            let decoded = decode_and_verify(&proof, root, size, offset, len as u64).unwrap();
            assert_eq!(decoded, block, "block@{offset} roundtrip 不一致");
            if size == 0 {
                break;
            }
            offset += len as u64;
        }
    }

    #[test]
    fn roundtrip_single_block() {
        roundtrip_all_blocks(&data_of(100 * 1024)); // < 1 block
    }

    #[test]
    fn roundtrip_multi_block_aligned() {
        roundtrip_all_blocks(&data_of(CHUNK_SIZE * 3)); // 恰好 3 块
    }

    #[test]
    fn roundtrip_tail_unaligned() {
        // 尾部非对齐（2 整块 + 88KiB 零头，且非 16KiB 整数倍）。
        roundtrip_all_blocks(&data_of(CHUNK_SIZE * 2 + 88 * 1024 + 123));
    }

    #[test]
    fn tampered_block_is_rejected() {
        let data = data_of(CHUNK_SIZE * 2 + 50 * 1024);
        let (root, outboard) = build_outboard(&data);
        let size = data.len() as u64;
        // 取第 2 块（尾块）生成 proof，篡改一字节 → decode 必败。
        let offset = CHUNK_SIZE as u64;
        let len = (size - offset) as usize;
        let block = &data[offset as usize..];
        let mut proof = encode_proof(&outboard, root, size, offset, block).unwrap();
        // 找到一个 leaf 数据字节翻转（切片尾部大概率落在 leaf 区）。
        let last = proof.len() - 1;
        proof[last] ^= 0xFF;
        let err = decode_and_verify(&proof, root, size, offset, len as u64).unwrap_err();
        assert!(
            err.to_string().contains("bao 逐块验证失败"),
            "篡改块必须被拒: {err}"
        );
    }

    #[test]
    fn wrong_root_is_rejected() {
        let data = data_of(CHUNK_SIZE);
        let (root, outboard) = build_outboard(&data);
        let size = data.len() as u64;
        let proof = encode_proof(&outboard, root, size, 0, &data).unwrap();
        // 用错误 root（另一份数据的 hash）解码 → 验签失败。
        let wrong_root = blake3::hash(b"different");
        assert!(decode_and_verify(&proof, wrong_root, size, 0, size).is_err());
    }

    #[test]
    fn empty_file_roundtrips() {
        roundtrip_all_blocks(&[]);
    }

    /// 宿主超长返回（违反 read_source_chunk 契约）必须响错拒收，
    /// 而不是把非法长度送进 blake3 的 subtree hasher（panic）。
    #[tokio::test]
    async fn overlong_host_read_is_rejected_not_panic() {
        let data = data_of(98061);
        let source: Arc<dyn FileAccess> = Arc::new(MockSource {
            data,
            ignore_len: true,
        });
        let err = build_outboard_from_source(&source, &FileSourceId("x".into()), 98061)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("违反契约"), "应响契约错误: {err}");
    }

    /// 非 CHUNK_SIZE 对齐的 offset（16KiB，resume 场景可出现）也必须可 roundtrip。
    /// 尺寸取 2026-07 图片传输事故的真实值（98061 = 16384 + 81677）。
    #[test]
    fn roundtrip_from_16kib_offset() {
        let data = data_of(98061);
        let (root, outboard) = build_outboard(&data);
        let size = data.len() as u64;
        let offset = 16384u64;
        let block = &data[offset as usize..];
        let proof = encode_proof(&outboard, root, size, offset, block).unwrap();
        let decoded = decode_and_verify(&proof, root, size, offset, block.len() as u64).unwrap();
        assert_eq!(decoded, block);
    }

    #[test]
    fn checksum_hex_roundtrips_as_root() {
        let data = data_of(CHUNK_SIZE + 1);
        let (root, _) = build_outboard(&data);
        let hex = root.to_hex().to_string();
        assert_eq!(root_from_checksum(&hex).unwrap(), root);
    }
}
