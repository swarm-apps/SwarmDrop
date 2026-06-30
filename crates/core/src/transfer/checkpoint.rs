//! 断点续传 checkpoint 的 bitmap / range 纯函数。
//!
//! 这些都是无副作用的位图数学（标记 / 统计 / 转 ranges / 校验），从 receiver 数据面
//! 循环里抽出来集中，供接收侧落盘 checkpoint 与 resume 协商共用，避免散落重复。

use std::collections::HashMap;

use crate::protocol::{FileInfo, FileRange};
use crate::transfer::{calc_total_chunks, CHUNK_SIZE};
use crate::{AppError, AppResult};

/// Finish 前校验所有文件的 bitmap 都已收满（空文件视为已完成）。
pub(crate) fn ensure_files_complete(
    files: &[FileInfo],
    bitmaps: &HashMap<u32, Vec<u8>>,
) -> AppResult<()> {
    if let Some(range) = first_missing_range(files, bitmaps) {
        return Err(AppError::Transfer(format!(
            "Finish 前仍有未完成数据: file_id={}, offset={}, length={}",
            range.file_id, range.offset, range.length
        )));
    }
    Ok(())
}

/// 返回首个未完成 chunk 的 range（用于完整性校验与续传补洞）。
pub(crate) fn first_missing_range(
    files: &[FileInfo],
    bitmaps: &HashMap<u32, Vec<u8>>,
) -> Option<FileRange> {
    for file in files {
        if file.size == 0 {
            continue;
        }

        let total_chunks = calc_total_chunks(file.size);
        let bitmap = bitmaps.get(&file.file_id);
        for chunk_index in 0..total_chunks {
            if bitmap
                .map(|bm| !is_chunk_completed(bm, chunk_index))
                .unwrap_or(true)
            {
                return Some(chunk_range(file, chunk_index));
            }
        }
    }
    None
}

/// 第 `chunk_index` 块在文件内的 range（末块按文件大小截断）。
pub(crate) fn chunk_range(file: &FileInfo, chunk_index: u32) -> FileRange {
    let offset = chunk_index as u64 * CHUNK_SIZE as u64;
    FileRange {
        file_id: file.file_id,
        offset,
        length: (file.size - offset).min(CHUNK_SIZE as u64),
    }
}

/// 校验入站 BlockData 的 range：不溢出、不越界、不超 chunk、按 chunk 对齐、非空文件非零长。
pub(crate) fn validate_block_range(file: &FileInfo, range: &FileRange) -> AppResult<()> {
    let end = range
        .offset
        .checked_add(range.length)
        .ok_or_else(|| AppError::Transfer("BlockData range 溢出".into()))?;
    if end > file.size {
        return Err(AppError::Transfer(format!(
            "BlockData range 超出文件大小: file_id={}, end={}, size={}",
            range.file_id, end, file.size
        )));
    }
    if range.length > CHUNK_SIZE as u64 {
        return Err(AppError::Transfer(format!(
            "BlockData range 超出 chunk 大小: {} > {}",
            range.length, CHUNK_SIZE
        )));
    }
    if file.size > 0 && !range.offset.is_multiple_of(CHUNK_SIZE as u64) {
        return Err(AppError::Transfer(format!(
            "BlockData offset 未按 chunk 对齐: {}",
            range.offset
        )));
    }
    if file.size > 0 && range.length == 0 {
        return Err(AppError::Transfer("非空文件收到空 BlockData".into()));
    }
    Ok(())
}

/// 检查指定 chunk 是否已完成。
pub(crate) fn is_chunk_completed(bitmap: &[u8], chunk_index: u32) -> bool {
    let byte_idx = (chunk_index / 8) as usize;
    let bit_idx = chunk_index % 8;
    byte_idx < bitmap.len() && (bitmap[byte_idx] & (1 << bit_idx)) != 0
}

/// 标记指定 chunk 为已完成。
pub(crate) fn mark_chunk_completed(bitmap: &mut [u8], chunk_index: u32) {
    let byte_idx = (chunk_index / 8) as usize;
    let bit_idx = chunk_index % 8;
    if byte_idx < bitmap.len() {
        bitmap[byte_idx] |= 1 << bit_idx;
    }
}

/// 统计 bitmap 中已完成的 chunk 数（利用 popcount 加速）。
pub(crate) fn count_completed_in_bitmap(bitmap: &[u8], total_chunks: u32) -> u32 {
    let full_bytes = (total_chunks / 8) as usize;
    let remainder_bits = total_chunks % 8;

    let mut count: u32 = bitmap.iter().take(full_bytes).map(|b| b.count_ones()).sum();

    // 尾部不完整字节：仅统计有效位
    if remainder_bits > 0
        && let Some(&last_byte) = bitmap.get(full_bytes)
    {
        let mask = (1u8 << remainder_bits) - 1;
        count += (last_byte & mask).count_ones();
    }

    count
}

/// 根据 bitmap 计算已传输字节数（末块按实际大小计）。
pub(crate) fn bytes_from_bitmap(bitmap: &[u8], file_size: u64, total_chunks: u32) -> u64 {
    if file_size == 0 || total_chunks == 0 {
        return 0;
    }
    let chunk_size = CHUNK_SIZE as u64;
    let last_chunk_size = match file_size % chunk_size {
        0 => chunk_size,
        r => r,
    };

    let full_chunk_count = count_completed_in_bitmap(bitmap, total_chunks.saturating_sub(1));
    let last_chunk_done = is_chunk_completed(bitmap, total_chunks - 1);

    full_chunk_count as u64 * chunk_size + if last_chunk_done { last_chunk_size } else { 0 }
}

/// 把 bitmap 转成已落盘的连续 (offset, length) ranges（相邻块合并）。
pub(crate) fn ranges_from_bitmap(bitmap: &[u8], file_size: u64, total_chunks: u32) -> Vec<(u64, u64)> {
    let mut ranges = Vec::new();
    let mut current: Option<(u64, u64)> = None;

    for chunk_index in 0..total_chunks {
        if !is_chunk_completed(bitmap, chunk_index) {
            if let Some(range) = current.take() {
                ranges.push(range);
            }
            continue;
        }

        let offset = chunk_index as u64 * CHUNK_SIZE as u64;
        let length = (file_size.saturating_sub(offset)).min(CHUNK_SIZE as u64);
        if length == 0 {
            continue;
        }
        match current.as_mut() {
            Some((_, len)) => *len += length,
            None => current = Some((offset, length)),
        }
    }

    if let Some(range) = current {
        ranges.push(range);
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造仅包含有效 chunk 位的完整 bitmap（仅测试使用）
    fn completed_bitmap(total_chunks: u32) -> Vec<u8> {
        let mut bitmap = vec![0u8; (total_chunks as usize).div_ceil(8)];
        for chunk_index in 0..total_chunks {
            mark_chunk_completed(&mut bitmap, chunk_index);
        }
        bitmap
    }

    #[test]
    fn completed_bitmap_marks_only_valid_chunks() {
        let bitmap = completed_bitmap(10);

        assert_eq!(bitmap.len(), 2);
        assert_eq!(count_completed_in_bitmap(&bitmap, 10), 10);
        assert!(!is_chunk_completed(&bitmap, 10));
    }

    #[test]
    fn bytes_from_bitmap_counts_partial_last_chunk_once() {
        let file_size = CHUNK_SIZE as u64 * 2 + 7;
        let total_chunks = calc_total_chunks(file_size);
        let mut bitmap = vec![0u8; (total_chunks as usize).div_ceil(8)];

        mark_chunk_completed(&mut bitmap, 0);
        mark_chunk_completed(&mut bitmap, 2);

        assert_eq!(
            bytes_from_bitmap(&bitmap, file_size, total_chunks),
            CHUNK_SIZE as u64 + 7
        );
    }

    #[test]
    fn ranges_from_bitmap_merges_contiguous_chunks() {
        let file_size = CHUNK_SIZE as u64 * 3 + 9;
        let total_chunks = calc_total_chunks(file_size);
        let mut bitmap = vec![0u8; (total_chunks as usize).div_ceil(8)];
        mark_chunk_completed(&mut bitmap, 0);
        mark_chunk_completed(&mut bitmap, 1);
        mark_chunk_completed(&mut bitmap, 3);

        assert_eq!(
            ranges_from_bitmap(&bitmap, file_size, total_chunks),
            vec![(0, CHUNK_SIZE as u64 * 2), (CHUNK_SIZE as u64 * 3, 9)]
        );
    }

    #[test]
    fn ensure_files_complete_rejects_missing_nonzero_chunk() {
        let file = FileInfo {
            file_id: 7,
            name: "demo.bin".into(),
            relative_path: "demo.bin".into(),
            size: CHUNK_SIZE as u64 + 1,
            checksum: "hash".into(),
        };
        let mut bitmaps = HashMap::new();
        let mut bitmap = vec![0u8; (calc_total_chunks(file.size) as usize).div_ceil(8)];
        mark_chunk_completed(&mut bitmap, 0);
        bitmaps.insert(file.file_id, bitmap);

        let err = ensure_files_complete(&[file], &bitmaps).unwrap_err();

        assert!(
            err.to_string().contains("Finish 前仍有未完成数据"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn ensure_files_complete_accepts_complete_bitmap() {
        let file = FileInfo {
            file_id: 8,
            name: "complete.bin".into(),
            relative_path: "complete.bin".into(),
            size: CHUNK_SIZE as u64 + 1,
            checksum: "hash".into(),
        };
        let mut bitmaps = HashMap::new();
        bitmaps.insert(file.file_id, completed_bitmap(calc_total_chunks(file.size)));

        assert!(ensure_files_complete(&[file], &bitmaps).is_ok());
    }

    #[test]
    fn ensure_files_complete_treats_zero_size_file_as_complete() {
        let file = FileInfo {
            file_id: 9,
            name: "empty.txt".into(),
            relative_path: "empty.txt".into(),
            size: 0,
            checksum: "hash".into(),
        };

        assert!(ensure_files_complete(&[file], &HashMap::new()).is_ok());
    }
}
