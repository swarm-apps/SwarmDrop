//! 发送方 prepare 阶段：流式 BLAKE3 hash + 进度事件推送。
//!
//! 这里只挂载 `TransferManager::prepare` 一个方法，结构体定义和其他生命周期方法
//! 仍在 [`crate::manager`]。

use n0_future::time::Instant;

use uuid::Uuid;

use crate::events::TransferEvent;
use crate::manager::{PreparedFile, PreparedTransfer, TransferManager};
use crate::progress::PrepareProgressEvent;
use crate::{AppError, AppResult};
use crate::{CHUNK_SIZE, HostEnumeratedFile, calc_total_chunks};

impl TransferManager {
    /// 准备发送：流式 BLAKE3 hash + 进度事件推送
    ///
    /// - `prepared_id` 由 caller 生成，便于 host 关联进度推送通道
    /// - hash 计算通过 `FileAccess::read_source_chunk` 走，平台无关
    /// - PrepareProgress 事件按 200ms 节流推送，避免大文件刷屏
    pub async fn prepare(
        &self,
        prepared_id: Uuid,
        entries: Vec<HostEnumeratedFile>,
    ) -> AppResult<PreparedTransfer> {
        if entries.is_empty() {
            return Err(AppError::Transfer("文件列表为空".into()));
        }

        let total_files = entries.len() as u32;
        let total_bytes: u64 = entries.iter().map(|e| e.size).sum();
        let mut files = Vec::with_capacity(entries.len());
        let mut completed_bytes: u64 = 0;
        let mut last_emit = Instant::now();
        const PROGRESS_THROTTLE: std::time::Duration = std::time::Duration::from_millis(200);

        for (idx, entry) in entries.into_iter().enumerate() {
            let file_id = idx as u32;
            let mut hasher = blake3::Hasher::new();

            let total_chunks = calc_total_chunks(entry.size);
            for chunk_idx in 0..total_chunks {
                let offset = chunk_idx as u64 * CHUNK_SIZE as u64;
                let remaining = entry.size.saturating_sub(offset);
                let length = (remaining as usize).min(CHUNK_SIZE);
                if length == 0 && entry.size != 0 {
                    break;
                }
                let chunk = self
                    .file_access
                    .read_source_chunk(&entry.source_id, offset, length)
                    .await?;
                // length 已按 entry.size 预算，长度不符 = 宿主违约或文件被外部修改。
                // 静默吞掉会算出错误 checksum，拖到接收端验签才暴露，这里提前响错。
                if chunk.len() != length {
                    return Err(AppError::Transfer(format!(
                        "read_source_chunk 返回长度异常: 请求 {length}B@{offset}，得到 {}B ({})",
                        chunk.len(),
                        entry.relative_path
                    )));
                }
                hasher.update(&chunk);
                let bytes_in_file = offset + chunk.len() as u64;

                let now = Instant::now();
                if now.duration_since(last_emit) >= PROGRESS_THROTTLE {
                    last_emit = now;
                    let _ = self
                        .events
                        .emit(TransferEvent::PrepareProgress {
                            event: PrepareProgressEvent {
                                prepared_id,
                                current_file: entry.name.clone(),
                                completed_files: file_id,
                                total_files,
                                bytes_hashed: completed_bytes + bytes_in_file,
                                total_bytes,
                            },
                        })
                        .await;
                }
            }

            let checksum = hasher.finalize().to_hex().to_string();
            completed_bytes += entry.size;

            // 逐块验签的 bao outboard：流式建（内存有界），root 必等于上面的扁平 blake3
            // checksum（16KiB chunk group 不改 root）。debug 下核验两者一致。
            let (root, outboard) = crate::bao::build_outboard_from_source(
                &self.file_access,
                &entry.source_id,
                entry.size,
            )
            .await?;
            debug_assert_eq!(
                root.to_hex().to_string(),
                checksum,
                "bao root 必须等于扁平 blake3 checksum"
            );

            files.push(PreparedFile {
                file_id,
                name: entry.name,
                relative_path: entry.relative_path,
                source_id: entry.source_id,
                size: entry.size,
                checksum,
                outboard,
            });
        }

        // 最终完成事件（不受节流限制）
        let _ = self
            .events
            .emit(TransferEvent::PrepareProgress {
                event: PrepareProgressEvent {
                    prepared_id,
                    current_file: String::new(),
                    completed_files: total_files,
                    total_files,
                    bytes_hashed: total_bytes,
                    total_bytes,
                },
            })
            .await;

        let prepared = PreparedTransfer {
            prepared_id,
            files,
            total_size: total_bytes,
            created_at: Instant::now(),
        };

        self.prepared.insert(prepared.prepared_id, prepared.clone());
        Ok(prepared)
    }
}
