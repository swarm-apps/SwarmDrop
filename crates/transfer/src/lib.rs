//! swarmdrop-transfer：文件传输域（从 swarmdrop-core 迁出）。
//!
//! 经端口 trait 依赖倒置：持久化走 [`store::SessionStore`]/[`store::InboxStore`]，
//! 配对目录走 [`peer::PeerDirectory`]，事件发射走 [`events::TransferEventSink`]，
//! 生命周期清理走 [`runtime::TransferRuntime`]——本 crate **不依赖 sea-orm / pairing /
//! network 模块**，wasm 双 target 可编。协议控制面（transfer-ctrl / transfer-data）在
//! [`protocol`]。

pub mod actor;
pub mod bao;
pub mod coordinator;
pub mod epoch;
pub mod events;
pub mod failure;
pub mod flow;
pub mod inbox;
pub mod incoming;
pub mod manager;
pub mod peer;
pub mod policy;
pub(crate) mod probe;
pub mod progress;
pub mod protocol;
pub mod runtime;
pub mod store;
pub mod text_delivery;
pub mod text_service;
pub mod wire;

// ── 兼容层：迁出前这些模块内写的是 `crate::host::` / `crate::device::` /
//    `crate::error::` / `crate::{AppError, AppResult}`；下沉后统一解析到 swarmdrop-host，
//    避免逐处改路径。事件（CoreEvent/EventBus）不在 host——由 events 端口取代。
pub use swarmdrop_host as host;
pub use swarmdrop_host::{AppError, AppResult, device, error};

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::host::{FileAccess, FileSourceId};
use crate::store::ExpiredReceiverActor;

/// 传输分块大小：256 KiB。
pub const CHUNK_SIZE: usize = 256 * 1024;

/// 数据面流控窗口：发送方每写这么多块就停下等接收方确认（见
/// [`TransferDataFrame::Window`](crate::wire::data_frame::TransferDataFrame::Window)）。
///
/// 16 × 256 KiB = **4 MiB 在途上限**，是接收侧缓冲的硬约束推出来的，不是拍的：浏览器
/// WebRTC 适配层的读缓冲上限 16 MiB（`webrtc-p2p` 的 `DEFAULT_MAX_READ_BUFFER`），
/// 留 4× 余量吸收 bao proof 开销、帧头，以及「已进读缓冲但 Window 尚未回出」的瞬时堆积。
/// **调大它必须同时抬那个上限**，否则又会退回越限重置。
///
/// 吞吐代价是每窗一个 RTT：20 MB 只需 5 次往返，跨网 RTT 50 ms 也不过 0.25 s，而
/// 稳态吞吐由接收端的处理速率决定（发送端在等 Window 时接收端正忙着验签落盘），
/// 窗口本身不是瓶颈。
pub const WINDOW_CHUNKS: u32 = 16;

/// 遗留 suspended 接收会话的过期保留期（秒），默认 7 天。
///
/// 超过此时长仍未恢复的 recoverable suspended 接收会话，在启动清理时转 terminal
/// 并清理 `.part`，防止活动列表与磁盘临时文件无限堆积。两端一致。
pub const SUSPENDED_RECEIVE_RETENTION_SECS: u64 = 7 * 24 * 60 * 60;

/// 过期回收写进 `error_message` 列的判别码。
///
/// 两端的回收各自改自己的模型（native 是 sea-orm `ActiveModel`，Web 是 `entity::Model`），
/// 字段赋值没法共享；但「过期回收」这个判别码与它的天数参数是同一件事，散成两处
/// 硬编码时，改一处就成了静默的口径漂移。
pub fn expired_receive_reason(retention_secs: u64) -> crate::failure::FailureCode {
    crate::failure::FailureCode::SessionExpired {
        retention_days: (retention_secs / 86_400) as u32,
    }
}

/// `[offset, end)` 是否落在传输块边界上（尾块可以短）。
///
/// # 这条判据有四个消费者，必须只有一份
///
/// 两道协商阶段的关（**计划级**，一次计划校验一次）：
///
/// 1. 接收侧 `actor::receiver::ReceiverActor::validate_fetch_plan` —— 对端 Hello 里的计划；
/// 2. 发送侧 `flow::resume::validation::validate_fetch_plan` —— 对端 `ResumeCommit` 的计划。
///
/// 两道数据面的防御（**块级**，兜住协商没拦住的）：
///
/// 3. 接收侧 `actor::checkpoint::validate_block_range` —— 入站 `BlockData` 的 range；
/// 4. `bao::encode_proof` —— 生成证明前的前置条件。
///
/// 三者恒等，因为 [`bao::BLOCK_SIZE`](crate::bao::BLOCK_SIZE) 就是从 [`CHUNK_SIZE`]
/// 推导的（每个传输块恰好一个验签叶子）。**chunk group 与传输块对齐之前它们不恒等**
/// ——那时 group 是 16 KiB，非对齐输入有 16 倍冗余可以蒙混过去，于是 (2) 一直缺着都没
/// 人发现：对端提交一个非对齐 offset 的续传计划，本端会接受、建 actor、开流、读盘，
/// 一路走到 (3) 才抛一句泛型错误 → abort → Interrupted → 对端拿同一个计划再来一次。
/// 冗余归零之后，那个洞必须在协商阶段就堵上。
pub fn is_chunk_aligned_range(offset: u64, end: u64, file_size: u64) -> bool {
    let chunk = CHUNK_SIZE as u64;
    offset.is_multiple_of(chunk) && (end.is_multiple_of(chunk) || end == file_size)
}

/// 计算文件总分块数。
pub fn calc_total_chunks(file_size: u64) -> u32 {
    if file_size == 0 {
        return 1;
    }
    file_size.div_ceil(CHUNK_SIZE as u64) as u32
}

/// 把一条 `fetch_plan` range 展开成逐个传输块的 `(offset, length)`。
///
/// **`length == 0` 产出恰好一个空块**，那是零字节文件唯一的块（`full_fetch_plan` 会为
/// `size == 0` 的文件给出一条长度为 0 的 range，对端据此建 sink 并发布）。
///
/// 这条规则在整个定义域上成立，**前提是非空文件的零长 range 已在协商阶段被拒**——
/// 收发两侧的 `validate_fetch_plan` 各有一条 `file.size > 0 && length == 0` ⇒ 拒。
/// 少了那道关，这里就会为一个非空文件产出一个空块，接收端收到后既写不进东西也推不动
/// bitmap，会话卡在「对端说发完了、位图却不完整」上。
///
/// `offset + length` 不得溢出（同样由协商阶段的校验保证）——**故意不 `pub`**：
/// 前置条件与它的守卫都住在本 crate 内，暴露出去就成了一条只能靠文档维系的约定。
pub(crate) fn blocks_in_range(offset: u64, length: u64) -> impl Iterator<Item = (u64, usize)> {
    let chunk = CHUNK_SIZE as u64;
    // 零长 ⇒ `div_ceil` 给 0，而那正是必须产出一个空块的情形，单列。
    let count = if length == 0 {
        1
    } else {
        length.div_ceil(chunk)
    };
    (0..count).map(move |index| {
        let consumed = index * chunk;
        let len = (length - consumed).min(chunk) as usize;
        (offset + consumed, len)
    })
}

/// 尽力清理过期回收会话遗留的 `.part` 文件。
///
/// 重启后原 `ReceiverActor`（及其 `created_sinks`）已不存在，故按 DB 文件元数据
/// 重建 sink（`open_or_create_sink`）再删除（`cleanup_sink`）。两端各传本端
/// `FileAccess`。失败仅告警不阻断——文件可能已被用户删除或路径不可达。
pub async fn cleanup_expired_part_files(
    file_access: &Arc<dyn FileAccess>,
    reaped: &[ExpiredReceiverActor],
) {
    for session in reaped {
        for meta in &session.files {
            let relative_path = meta.relative_path.clone();
            match file_access.open_or_create_sink(meta.clone()).await {
                Ok(sink_id) => {
                    if let Err(e) = file_access.cleanup_sink(&sink_id).await {
                        warn!(
                            "清理过期 .part 失败: session={}, file={relative_path}, {e}",
                            session.session_id
                        );
                    }
                }
                Err(e) => warn!(
                    "重建过期 .part sink 失败: session={}, file={relative_path}, {e}",
                    session.session_id
                ),
            }
        }
    }
}

/// Host 提供的待传输文件条目（host-agnostic）。
///
/// 桌面端 / RN 端在自己的 file picker / scanner 中产出 `FileSourceId`，
/// core 不关心它如何编码，只通过 `FileAccess` trait 读取。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostEnumeratedFile {
    pub source_id: FileSourceId,
    pub name: String,
    pub relative_path: String,
    pub size: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blocks(offset: u64, length: u64) -> Vec<(u64, usize)> {
        blocks_in_range(offset, length).collect()
    }

    #[test]
    fn splits_a_range_on_chunk_boundaries() {
        let chunk = CHUNK_SIZE as u64;
        assert_eq!(blocks(0, chunk), vec![(0, CHUNK_SIZE)]);
        assert_eq!(
            blocks(0, chunk * 3),
            vec![
                (0, CHUNK_SIZE),
                (chunk, CHUNK_SIZE),
                (chunk * 2, CHUNK_SIZE)
            ]
        );
        // 续传的 range 不从 0 起：块边界跟着 range 的起点走。
        assert_eq!(
            blocks(chunk * 2, chunk * 2),
            vec![(chunk * 2, CHUNK_SIZE), (chunk * 3, CHUNK_SIZE)]
        );
    }

    #[test]
    fn last_block_may_be_short() {
        let chunk = CHUNK_SIZE as u64;
        assert_eq!(
            blocks(0, chunk + 777),
            vec![(0, CHUNK_SIZE), (chunk, 777)],
            "尾块短于 CHUNK_SIZE 是合法的（文件末尾）"
        );
        assert_eq!(blocks(0, 1), vec![(0, 1)]);
    }

    /// 零长 range 产出**恰好一个**空块，而不是零个。
    ///
    /// 零个的后果是零字节文件永远等不到自己的块：接收端不会为它建 sink、也不会
    /// 发布，而 `ensure_files_complete` 对 `size == 0` 直接放行 —— 会话报完成，
    /// 文件却从未落地。
    #[test]
    fn zero_length_range_yields_exactly_one_empty_block() {
        assert_eq!(blocks(0, 0), vec![(0, 0)]);
    }

    /// 非零长 range 绝不产出空块。
    ///
    /// 空块会让接收端的 bitmap 推不动，而对端已经把这一块算作发过了。
    #[test]
    fn non_empty_range_never_yields_an_empty_block() {
        let chunk = CHUNK_SIZE as u64;
        for length in [1, chunk - 1, chunk, chunk + 1, chunk * 4 + 3] {
            assert!(
                blocks(0, length).iter().all(|(_, len)| *len > 0),
                "length={length} 产出了空块"
            );
        }
    }

    #[test]
    fn block_lengths_sum_back_to_the_range() {
        let chunk = CHUNK_SIZE as u64;
        for length in [1, chunk, chunk * 5 + 12_345] {
            let total: u64 = blocks(chunk * 7, length)
                .iter()
                .map(|(_, l)| *l as u64)
                .sum();
            assert_eq!(total, length, "切分不得增删字节");
        }
    }
}
