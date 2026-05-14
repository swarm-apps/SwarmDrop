//! 文件传输共享常量和工具函数。

/// 传输分块大小：256 KiB。
pub mod incoming;
pub mod progress;

pub const CHUNK_SIZE: usize = 256 * 1024;

/// 计算文件总分块数。
pub fn calc_total_chunks(file_size: u64) -> u32 {
    if file_size == 0 {
        return 1;
    }
    file_size.div_ceil(CHUNK_SIZE as u64) as u32
}
