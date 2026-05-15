//! 文件传输共享常量和工具函数。

pub mod crypto;
pub mod incoming;
pub mod manager;
pub mod progress;
pub mod receiver;
pub mod sender;

use serde::{Deserialize, Serialize};

use crate::host::FileSourceId;

/// 传输分块大小：256 KiB。
pub const CHUNK_SIZE: usize = 256 * 1024;

/// 计算文件总分块数。
pub fn calc_total_chunks(file_size: u64) -> u32 {
    if file_size == 0 {
        return 1;
    }
    file_size.div_ceil(CHUNK_SIZE as u64) as u32
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
