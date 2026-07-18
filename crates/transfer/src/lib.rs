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
pub mod flow;
pub mod incoming;
pub mod manager;
pub mod peer;
pub mod policy;
pub mod progress;
pub mod protocol;
pub mod runtime;
pub mod store;
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

/// 遗留 suspended 接收会话的过期保留期（秒），默认 7 天。
///
/// 超过此时长仍未恢复的 recoverable suspended 接收会话，在启动清理时转 terminal
/// 并清理 `.part`，防止活动列表与磁盘临时文件无限堆积。两端一致。
pub const SUSPENDED_RECEIVE_RETENTION_SECS: u64 = 7 * 24 * 60 * 60;

/// 计算文件总分块数。
pub fn calc_total_chunks(file_size: u64) -> u32 {
    if file_size == 0 {
        return 1;
    }
    file_size.div_ceil(CHUNK_SIZE as u64) as u32
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
