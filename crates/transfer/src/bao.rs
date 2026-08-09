//! bao-tree 逐块完整性验证。
//!
//! 补上「文件收完前每个块可验证」的能力（取代「续传信任对端」）。用 bao-tree 的
//! **库级 encode/decode 路径**——不手写 Merkle 验证（易错）。
//!
//! ## 选型：proof 携带完整 bao 切片，`BlockData.data` 置空
//!
//! 每个 [`BlockData`](crate::wire::data_frame::TransferDataFrame) 的 `proof` 字段直接放
//! [`encode_ranges_validated`] 产出的完整 bao 切片（交错的 Parent/Leaf），`data` 字段置空。
//! 接收端把整段喂 [`decode_ranges`]（root = `FileInfo.checksum` 解析回 blake3::Hash）——
//! decode 必然验签、无 skip 选项，验过即得明文块写盘。
//!
//! 为何不拆 Parent 进 proof、Leaf 进 data：库没有稳定的「拆/组交错流」公开迭代顺序 API，
//! 手动交错易错；而完整切片方案里 data 置空，叶子只在 proof 出现一次，**不产生 2x 冗余**
//! （wire 开销 ≈ 明文 + parents ≈ 0.3%）。两方案都不改 wire 布局（proof 是 opaque bytes）。
//!
//! ## root 就是 checksum，不是「对得上的另一个值」
//!
//! bao 树根 == 标准 blake3 整文件 hash（chunk group 只影响 outboard 深度，不影响 root）。
//! prepare **只读一遍源文件**，[`build_outboard_from_source_with_progress`] 同时产出
//! `(root, outboard)`，`FileInfo.checksum` 就取自这个 root——不是「另算一遍再断言相等」。
//!
//! 2026-08 之前是后者：prepare 先跑一遍扁平 `blake3::Hasher` 得 checksum、再跑一遍
//! 建 outboard，靠 `debug_assert_eq!` 互证（release 下不执行）。那一遍不只是白读，
//! 还让「两遍之间源文件被改」能产出互不匹配的 checksum 与 outboard，一路拖到接收端
//! 验签才炸、且归因指向网络。同源之后这类不一致在构造上不可能。

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
use crate::{AppError, AppResult, CHUNK_SIZE};

/// chunk group **恒等于** [`CHUNK_SIZE`]：每个传输块恰好一个叶子。
///
/// 这里从 `CHUNK_SIZE` 推导而不是写死一个 `from_chunk_log(n)`，是为了让「验签粒度 ==
/// 传输块粒度」在**构造上**成立——改了 `CHUNK_SIZE` 而忘了改这里，会编译期失败而不是
/// 悄悄退回一个不对齐的树。
///
/// 曾用 16KiB（iroh `IROH_BLOCK_SIZE` 同款），验签粒度比传输块细 16 倍。那 16 倍从来
/// 没有消费方——proof 与传输块本来就一一对应，本仓从未发出过 sub-`CHUNK_SIZE` 的验证
/// 请求；代价却是实打实的：outboard 大 16 倍、wire 上的 parent 开销大一倍多、构建时
/// 对 [`FileAccess::read_source_chunk`] 的调用次数大 16 倍（三端宿主都是**每次重新打开
/// 文件**，主导成本是调用次数而非字节数）。
///
/// # 改这个常量等于改 wire
///
/// proof 的树形状随之改变，旧端产出的 proof 在新端**第一个块**就验签失败 → 协议违规 →
/// 断流 → 恢复重试循环。必须同时 bump
/// [`TRANSFER_DATA_PROTOCOL`](crate::protocol::TRANSFER_DATA_PROTOCOL)，把不兼容前移到
/// 协商阶段（判据见 `protocol.rs` 该常量的 doc）。持久化的 outboard 也会随之作废，
/// 由 [`outboard_len`] 的长度判据自动识别，不需要数据迁移。
///
/// # 单向门
///
/// 验签粒度是最小可独立验证单元。256KiB 是将来做 range 请求 / 部分文件预览 / 与
/// iroh-blobs 互通时的硬下限。
pub const BLOCK_SIZE: BlockSize = match BlockSize::from_bytes(CHUNK_SIZE as u64) {
    Some(bs) => bs,
    None => panic!("CHUNK_SIZE 必须是 ≥1024 的 2 的幂，否则无法作为 bao chunk group"),
};

/// 给定文件大小，当前 [`BLOCK_SIZE`] 下 outboard 的**确定性**字节长度。
///
/// # 长度就是格式版本号
///
/// outboard 不上 wire、但会落库供 resume 免重算。chunk group 一变，旧记录的字节仍然
/// 「非空且看起来合法」，喂进新树则每块 `ParentHashMismatch`——若失效判据写成
/// `is_empty()`，那条会话就**永久**续不上传：不 panic、不报错、重算分支也永不触发。
///
/// 用长度做判据同时解决三件事：治好格式变更留下的存量毒数据、让将来再调 chunk group
/// 不需要写迁移、以及让 ≤`CHUNK_SIZE` 的文件（期望长度恒为 0）不再被 `is_empty()`
/// 误判成缺失而每次 resume 白读一遍整文件。
pub fn outboard_len(size: u64) -> u64 {
    BaoTree::new(size, BLOCK_SIZE).outboard_size()
}

/// 一份持久化下来的 outboard 是否**还能用**（格式与当前 [`BLOCK_SIZE`] 相符）。
///
/// 判据就是 [`outboard_len`]——见那里关于「长度即格式版本号」的说明。调用点用这个
/// 而不是自己算，是为了让意图（「这份还能用吗」）而非算术出现在 resume 逻辑里。
pub fn is_outboard_usable(outboard: &[u8], size: u64) -> bool {
    outboard.len() as u64 == outboard_len(size)
}

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

/// 流式构建期间的读取进度回调。
///
/// bao 的构建循环在库内部，唯一能挂进去的地方是传给它的 reader。这个 trait 让本模块
/// 对事件系统保持无知：它只报「本文件已读到第几字节」，节流与事件形态由调用方决定。
///
/// 形态对齐 [`AsyncSliceReader`]（RPITIT、无 `Send` bound），因此 wasm 下同样适用。
pub trait ReadProgress {
    /// `bytes_in_file` 是**本文件**已读的累计字节数，单调不减，末次等于文件大小。
    ///
    /// 实现方 MUST NOT 在这里把错误上抛——它没有返回值正是为此。事件总线的一次抖动
    /// 不该中断一次正常的 outboard 构建。
    fn on_read(&mut self, bytes_in_file: u64) -> impl std::future::Future<Output = ()>;
}

/// 不上报的 [`ReadProgress`]，供 resume 回填等无 UI 关联的构建路径。
struct NoProgress;

impl ReadProgress for NoProgress {
    async fn on_read(&mut self, _bytes_in_file: u64) {}
}

/// 从 host 的 [`FileAccess`] **流式**构建 post-order outboard（内存有界，不整文件入内存）。
///
/// 经 iroh-io 的 [`AsyncSliceReader`] 适配 async 分块读——避免「async FileAccess ↔ sync
/// outboard 构建」的桥接。返回 `(root, outboard_bytes)`，`root` == 标准 blake3 ==
/// `FileInfo.checksum`。
///
/// 这条**不报进度**，供 resume 回填缺失 outboard 时使用。prepare 走
/// [`build_outboard_from_source_with_progress`]——两个入口而非一个 `Option` 参数，是为了
/// 不让 resume 被迫编造一个 `prepared_id`（那会让 UI 收到一条没有对应 prepare 流程的
/// 进度事件）。
///
/// 与 [`build_outboard`]（sync in-memory，供单测/小数据）产出**同序**（post-order），
/// 故 [`encode_proof`] 用同一个 [`PostOrderOutboard`] 重建即可，无论哪条路构建。
pub async fn build_outboard_from_source(
    file_access: &Arc<dyn FileAccess>,
    source_id: &FileSourceId,
    size: u64,
) -> AppResult<(blake3::Hash, Vec<u8>)> {
    build_outboard_from_source_with_progress(
        file_access,
        source_id,
        size,
        &source_id.0,
        &mut NoProgress,
    )
    .await
}

/// 同 [`build_outboard_from_source`]，但每读到一段就调一次 `progress`。
///
/// `label` 只用于错误消息的可归因（prepare 传 `relative_path`）。
///
/// # 为什么进度能挂在这里
///
/// bao 在 outboard 构建这条路径上把 reader 当**不可 seek 的流**用：
/// `CreateOutboard::create` 先取 `size()`，再把 reader 包进 `std::io::Cursor` 当
/// `AsyncStreamReader`（iroh-io 的原话是「A non seekable reader, e.g. a network socket」），
/// `outboard_impl` 对它的唯一调用是 Leaf 分支的 `read_bytes_exact`。于是 offset 从 0
/// 严格单调递增、每次不超过一个 chunk group、末次为精确剩余、每字节只读一次——进度
/// 天然单调，且累加返回长度就等于已读字节数。
///
/// **这是 bao-tree 的实现事实，不是它承诺的契约**（`AsyncSliceReader` 的文档明说自己是
/// 随机读接口）。`records_sequential_forward_reads` 那条护栏测试是它唯一的警报。
pub async fn build_outboard_from_source_with_progress<P: ReadProgress>(
    file_access: &Arc<dyn FileAccess>,
    source_id: &FileSourceId,
    size: u64,
    label: &str,
    progress: &mut P,
) -> AppResult<(blake3::Hash, Vec<u8>)> {
    let reader = ProgressReader {
        inner: FileAccessReader {
            file_access: file_access.clone(),
            source_id: source_id.clone(),
            size,
            label: label.to_owned(),
        },
        progress,
        bytes_read: 0,
    };
    let ob = PostOrderOutboard::<Vec<u8>>::create(reader, BLOCK_SIZE)
        .await
        .map_err(|e| AppError::Transfer(format!("bao outboard 构建失败: {e}")))?;
    Ok((ob.root, ob.data))
}

/// [`AsyncSliceReader`] 适配层：把 bao outboard 构建的 async 分块读映射到 [`FileAccess`]。
struct FileAccessReader {
    file_access: Arc<dyn FileAccess>,
    source_id: FileSourceId,
    size: u64,
    /// 仅用于错误消息的可归因（prepare 传 `relative_path`）。
    label: String,
}

impl AsyncSliceReader for FileAccessReader {
    async fn read_at(&mut self, offset: u64, len: usize) -> std::io::Result<Bytes> {
        let chunk = self
            .file_access
            .read_source_chunk(&self.source_id, offset, len)
            .await
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        // 契约防御，严格等长两边都拒：
        //
        // - **超长**会把非法长度喂进 bao 的 subtree hasher（blake3 直接 panic，
        //   2026-07 桌面宿主取整 offset 的事故形态）。响错优于截断——超长通常伴随
        //   offset 错位，截断会静默产出错误 hash。
        // - **短读**在这里同样是违约，尽管端口契约允许「越 EOF 返回空、尾部截断」：
        //   outboard 构建的叶子范围被 bao 依 `size()` clamp 过（`leaf_byte_ranges3`），
        //   `offset + len <= size` 恒成立，所以等长是可断言的。若不拒，短读会退化成
        //   bao 包裹的 `UnexpectedEof`，丢掉「哪个文件、少了多少」这两条归因信息。
        //   `records_sequential_forward_reads` 那条护栏测试守着这个前提。
        if chunk.len() != len {
            // 措辞不武断归因：**短读有两种来路**——宿主实现违约，或者源文件在
            // `scan_sources` 与 `prepare` 之间被截短（移动端 SAF 源报了个过期的 size 也
            // 算）。第二种不是宿主的错，把它说成「违反契约」会把排查引向适配层。
            return Err(std::io::Error::other(format!(
                "读取长度与预期不符（源文件可能已变更，或宿主 read_source_chunk 违反契约）: \
                 请求 {len}B@{offset}，返回 {}B ({})",
                chunk.len(),
                self.label
            )));
        }
        Ok(Bytes::from(chunk))
    }

    async fn size(&mut self) -> std::io::Result<u64> {
        Ok(self.size)
    }
}

/// 在任意 [`AsyncSliceReader`] 上叠一层读取进度上报。
///
/// 依赖顺序读（见 [`build_outboard_from_source_with_progress`] 的说明）：`bytes_read`
/// 累加的是**返回长度**，因此只有在「每字节只读一次」时才等于已读进度。
struct ProgressReader<'a, R, P> {
    inner: R,
    progress: &'a mut P,
    bytes_read: u64,
}

impl<R: AsyncSliceReader, P: ReadProgress> AsyncSliceReader for ProgressReader<'_, R, P> {
    async fn read_at(&mut self, offset: u64, len: usize) -> std::io::Result<Bytes> {
        let bytes = self.inner.read_at(offset, len).await?;
        self.bytes_read += bytes.len() as u64;
        self.progress.on_read(self.bytes_read).await;
        Ok(bytes)
    }

    async fn size(&mut self) -> std::io::Result<u64> {
        self.inner.size().await
    }
}

/// 发送端：为 `[offset, offset+block.len())` 生成 bao 证明切片。
///
/// `block` 是该 range 的明文；`outboard_bytes`/`root`/`file_size` 描述整棵树。返回的切片
/// 是交错的 Parent/Leaf，接收端 [`decode_and_verify`] 独立可验。
///
/// # 块必须落在 chunk group 边界上
///
/// `encode_ranges_validated` 是按**整个叶子**读的（`read_exact_at(start_chunk.to_bytes(), ..)`），
/// 而这里喂给它的 [`OffsetReadAt`] 只持有 `block` 那一段字节。所以起点必须对齐，终点
/// 要么对齐、要么就是文件末尾（尾叶子可以短）。中间可以跨任意多个整叶子。
///
/// 曾经 chunk group 是 16KiB、传输块是 256KiB，于是有 16 倍的对齐冗余，违规输入大多
/// 「碰巧能跑」。两者相等之后冗余归零，而违规的失败形态是 `read_at 越过块起点` 或
/// `UnexpectedEof`——后者与真实 IO 故障无从区分。故在此显式校验。
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
    let end = offset + block.len() as u64;
    // 两条判据分开报：越界和不对齐是两种毛病，混在一句话里会把「length 算多了」的排查
    // 引向对齐。
    if end > file_size {
        return Err(AppError::Transfer(format!(
            "块越过文件末尾: [{offset}, {end}) / 文件 {file_size}B"
        )));
    }
    // 与接收侧的入站校验、发送侧的续传计划校验共用同一条判据（三者恒等，因为
    // BLOCK_SIZE 就是从 CHUNK_SIZE 推导的）。这里是最后一道，前两道在协商阶段。
    if !crate::is_chunk_aligned_range(offset, end, file_size) {
        return Err(AppError::Transfer(format!(
            "块未落在 chunk group 边界: [{offset}, {end})，group {}B",
            BLOCK_SIZE.bytes()
        )));
    }
    let tree = BaoTree::new(file_size, BLOCK_SIZE);
    let outboard = PostOrderOutboard {
        root,
        tree,
        data: outboard_bytes,
    };
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
        async fn delete_finalized_file(&self, _uri: &str) -> AppResult<()> {
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
        async fn open_or_create_sink(&self, _m: HostFileMetadata) -> AppResult<FileSinkId> {
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
    ///
    /// 尺寸必须**大于一个 chunk group**：单叶子树下 bao 只发一次被 clamp 到文件大小的
    /// 请求，`ignore_len` 的 mock 恰好返回等长，这条分支就永远走不到。
    #[tokio::test]
    async fn overlong_host_read_is_rejected_not_panic() {
        let size = CHUNK_SIZE + 12345;
        let source: Arc<dyn FileAccess> = Arc::new(MockSource {
            data: data_of(size),
            ignore_len: true,
        });
        let err = build_outboard_from_source(&source, &FileSourceId("x".into()), size as u64)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("违反契约"), "应响契约错误: {err}");
    }

    /// 宿主短读同样是违约：outboard 构建的请求已被 bao 依 `size()` clamp 过，
    /// `offset + len <= size` 恒成立，等长可断言。不拒的话会退化成 bao 包裹的
    /// `UnexpectedEof`，丢掉「哪个文件、少了多少」。
    #[tokio::test]
    async fn short_host_read_is_rejected_with_attribution() {
        struct ShortSource(Vec<u8>);

        #[async_trait::async_trait]
        impl FileAccess for ShortSource {
            async fn source_metadata(&self, _s: &FileSourceId) -> AppResult<HostFileMetadata> {
                unreachable!()
            }
            async fn delete_finalized_file(&self, _uri: &str) -> AppResult<()> {
                unreachable!()
            }
            async fn read_source_chunk(
                &self,
                _s: &FileSourceId,
                offset: u64,
                len: usize,
            ) -> AppResult<Vec<u8>> {
                // 每次少给一字节。
                let start = offset as usize;
                let end = (start + len.saturating_sub(1)).min(self.0.len());
                Ok(self.0[start..end].to_vec())
            }
            async fn create_sink(&self, _m: HostFileMetadata) -> AppResult<FileSinkId> {
                unreachable!()
            }
            async fn open_or_create_sink(&self, _m: HostFileMetadata) -> AppResult<FileSinkId> {
                unreachable!()
            }
            async fn write_sink_chunk(
                &self,
                _s: &FileSinkId,
                _o: u64,
                _d: Vec<u8>,
            ) -> AppResult<()> {
                unreachable!()
            }
            async fn finalize_sink(&self, _s: &FileSinkId) -> AppResult<FinalizedSink> {
                unreachable!()
            }
        }

        let size = CHUNK_SIZE + 999;
        let source: Arc<dyn FileAccess> = Arc::new(ShortSource(data_of(size)));
        let err = build_outboard_from_source_with_progress(
            &source,
            &FileSourceId("x".into()),
            size as u64,
            "photos/a.jpg",
            &mut NoProgress,
        )
        .await
        .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("读取长度与预期不符"), "应响长度错误: {msg}");
        assert!(msg.contains("photos/a.jpg"), "错误必须可归因到文件: {msg}");
    }

    /// chunk group 与 `CHUNK_SIZE` 相等后，非对齐 offset 从「有 16 倍冗余、多半碰巧能跑」
    /// 变成硬前提，必须被**明确**拒绝——而不是退化成 `read_at 越过块起点` 或
    /// `UnexpectedEof`（后者与真实 IO 故障无从区分）。
    ///
    /// 尺寸取 2026-07 图片传输事故的真实值（98061 = 16384 + 81677）。生产路径上
    /// 这种 offset 不可达（fetch_plan 的 offset 恒为 `CHUNK_SIZE` 倍数，且接收端
    /// `validate_block_range` 早就会拒），这条守的是「拒得清楚」而不是「能跑」。
    #[test]
    fn unaligned_offset_is_rejected_explicitly() {
        let data = data_of(98061);
        let (root, outboard) = build_outboard(&data);
        let size = data.len() as u64;
        let offset = 16384u64;
        let err =
            encode_proof(&outboard, root, size, offset, &data[offset as usize..]).unwrap_err();
        assert!(
            err.to_string().contains("chunk group 边界"),
            "非对齐 offset 必须被明确拒绝: {err}"
        );
    }

    /// 反面：块跨多个整叶子且不到 EOF，是**合法**输入，校验不许误报。
    #[test]
    fn multi_leaf_block_is_accepted() {
        let data = data_of(CHUNK_SIZE * 4);
        let (root, outboard) = build_outboard(&data);
        let size = data.len() as u64;
        let block = &data[0..CHUNK_SIZE * 3];
        let proof = encode_proof(&outboard, root, size, 0, block).unwrap();
        let decoded = decode_and_verify(&proof, root, size, 0, block.len() as u64).unwrap();
        assert_eq!(decoded, block);
    }

    /// 合并成一遍之后，`checksum` 不再有「另一遍扁平 hash」当对照物
    /// （原 `prepare.rs` 的 `debug_assert_eq!` 随之消失）。这条把那份保障从运行期
    /// 断言迁到单测里，边界覆盖 0 / 1 / group±1。
    #[tokio::test]
    async fn streaming_root_equals_flat_blake3_at_boundaries() {
        for size in [
            0usize,
            1,
            CHUNK_SIZE - 1,
            CHUNK_SIZE,
            CHUNK_SIZE + 1,
            CHUNK_SIZE * 2,
            CHUNK_SIZE * 2 + 1,
        ] {
            let data = data_of(size);
            let source: Arc<dyn FileAccess> = Arc::new(MockSource {
                data: data.clone(),
                ignore_len: false,
            });
            let (root, _) =
                build_outboard_from_source(&source, &FileSourceId("x".into()), size as u64)
                    .await
                    .unwrap_or_else(|e| panic!("size={size} 构建失败: {e}"));
            assert_eq!(
                root,
                blake3::hash(&data),
                "size={size} 的流式 root 必须等于扁平 blake3"
            );
        }
    }

    /// # 护栏：bao 的读取是顺序前进的
    ///
    /// **这条红了，说明 bao-tree 换了读取策略，本仓两条正确性前提同时失效**：
    ///
    /// 1. [`ProgressReader`] 靠「累加返回长度」得出已读进度——乱序或重读会让进度虚高；
    /// 2. [`FileAccessReader`] 的严格等长判据靠「请求恒被 clamp 在 `size` 内」——
    ///    越界请求会让每个文件末尾误报宿主违约。
    ///
    /// 顺序性来自实现而非契约：`AsyncSliceReader` 的文档明说自己是随机读接口，是
    /// `CreateOutboard::create` 把它包进 `Cursor` 当流用才有了这个性质。所以它必须被
    /// 测试钉住，而不是靠读源码相信。体例照 `webrtc-p2p` 的 `udp_mux` 那组不变量测试。
    #[tokio::test]
    async fn records_sequential_forward_reads() {
        use std::sync::Mutex;

        struct RecordingSource {
            data: Vec<u8>,
            calls: Arc<Mutex<Vec<(u64, usize)>>>,
        }

        #[async_trait::async_trait]
        impl FileAccess for RecordingSource {
            async fn source_metadata(&self, _s: &FileSourceId) -> AppResult<HostFileMetadata> {
                unreachable!()
            }
            async fn delete_finalized_file(&self, _uri: &str) -> AppResult<()> {
                unreachable!()
            }
            async fn read_source_chunk(
                &self,
                _s: &FileSourceId,
                offset: u64,
                len: usize,
            ) -> AppResult<Vec<u8>> {
                self.calls.lock().unwrap().push((offset, len));
                let start = offset as usize;
                let end = (start + len).min(self.data.len());
                Ok(self.data.get(start..end).unwrap_or_default().to_vec())
            }
            async fn create_sink(&self, _m: HostFileMetadata) -> AppResult<FileSinkId> {
                unreachable!()
            }
            async fn open_or_create_sink(&self, _m: HostFileMetadata) -> AppResult<FileSinkId> {
                unreachable!()
            }
            async fn write_sink_chunk(
                &self,
                _s: &FileSinkId,
                _o: u64,
                _d: Vec<u8>,
            ) -> AppResult<()> {
                unreachable!()
            }
            async fn finalize_sink(&self, _s: &FileSinkId) -> AppResult<FinalizedSink> {
                unreachable!()
            }
        }

        // 多叶子 + 尾部零头，覆盖「末次为精确剩余」。
        let size = CHUNK_SIZE * 3 + 4321;
        let calls = Arc::new(Mutex::new(Vec::new()));
        let source: Arc<dyn FileAccess> = Arc::new(RecordingSource {
            data: data_of(size),
            calls: calls.clone(),
        });

        // 同时验证 ReadProgress 的上报是单调的、末次等于文件大小。
        struct Recorder(Vec<u64>);
        impl ReadProgress for Recorder {
            async fn on_read(&mut self, bytes_in_file: u64) {
                self.0.push(bytes_in_file);
            }
        }
        let mut progress = Recorder(Vec::new());

        build_outboard_from_source_with_progress(
            &source,
            &FileSourceId("x".into()),
            size as u64,
            "guard.bin",
            &mut progress,
        )
        .await
        .unwrap();

        let calls = calls.lock().unwrap();
        assert!(!calls.is_empty(), "至少要读一次");
        let group = BLOCK_SIZE.bytes();

        let mut expected_offset = 0u64;
        for &(offset, len) in calls.iter() {
            assert_eq!(offset, expected_offset, "offset 必须顺序前进，无跳跃无重读");
            assert!(
                len <= group,
                "单次请求 {len}B 超过一个 chunk group {group}B"
            );
            assert!(
                offset + len as u64 <= size as u64,
                "请求 [{offset}, {}) 越过文件末尾 {size}",
                offset + len as u64
            );
            expected_offset += len as u64;
        }
        assert_eq!(
            expected_offset, size as u64,
            "累计读取长度必须恰好等于文件大小（每字节只读一次）"
        );

        assert!(
            progress.0.windows(2).all(|w| w[0] <= w[1]),
            "进度上报必须单调不减: {:?}",
            progress.0
        );
        assert_eq!(
            progress.0.last().copied(),
            Some(size as u64),
            "末次进度必须等于文件大小"
        );
    }

    /// chunk group 从 16KiB 提到 256KiB 后，存量 outboard 的字节**非空且看起来合法**，
    /// 只是树形状对不上。这正是 `is_empty()` 判据放不掉它、进而让那条会话永久续不上
    /// 传（每次 resume 都 ParentHashMismatch、重算分支永不触发）的原因。
    #[test]
    fn stale_outboard_from_smaller_chunk_group_is_rejected() {
        let size = CHUNK_SIZE as u64 * 4;
        // 16KiB chunk group 下的长度公式：(ceil(size / 16KiB) - 1) * 64
        let stale_len = (size.div_ceil(16 * 1024) - 1) * 64;
        let stale = vec![0u8; stale_len as usize];
        assert!(
            !stale.is_empty(),
            "存量 BLOB 非空——`is_empty()` 判据放不掉它，这条测试的前提就在这里"
        );
        assert!(
            !is_outboard_usable(&stale, size),
            "格式作废的存量必须判为不可用"
        );

        let (_, fresh) = build_outboard(&data_of(size as usize));
        assert!(is_outboard_usable(&fresh, size), "当前格式必须判为可用");
    }

    /// ≤ 一个 chunk group 的文件 outboard 恒为空且**合法**。旧的 `is_empty()` 判据会把
    /// 它们全判成缺失，于是每次 resume 都白读一遍整文件再存回一个空 vec。
    #[test]
    fn empty_outboard_is_usable_for_single_leaf_files() {
        for size in [0u64, 1, 100 * 1024, CHUNK_SIZE as u64] {
            assert_eq!(outboard_len(size), 0, "size={size} 应为单叶子树");
            assert!(
                is_outboard_usable(&[], size),
                "size={size} 的空 outboard 应可用"
            );
        }
        assert!(
            outboard_len(CHUNK_SIZE as u64 + 1) > 0,
            "跨叶子后 outboard 非空"
        );
    }

    #[test]
    fn checksum_hex_roundtrips_as_root() {
        let data = data_of(CHUNK_SIZE + 1);
        let (root, _) = build_outboard(&data);
        let hex = root.to_hex().to_string();
        assert_eq!(root_from_checksum(&hex).unwrap(), root);
    }
}
