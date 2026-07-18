//! 断点续传纯构造 / 计划生成（无 `self` / `TransferManager` 依赖）。
//!
//! manifest / checkpoint / fetch_plan / 续传 state 等的派生，均只读 entity model +
//! protocol 类型；与 [`super::validation`] 互相引用（plan 用其 `validate_checkpoint`，
//! validation 用本模块 `build_resume_manifest`），同 crate 双向模块引用无碍。

use std::collections::HashMap;

use crate::host::FileSourceId;
use crate::manager::{PreparedFile, ResumeFileInfo};
use crate::protocol::{FileCheckpoint, FileInfo, FileRange, ResumeRejectReason, ResumeReport};
use crate::{AppError, AppResult};
use crate::{CHUNK_SIZE, calc_total_chunks};

use super::validation::{resume_reject_message, validate_checkpoint};

/// 从文件记录构造 checkpoint（过渡近似：transferred_bytes 作单个连续 range；
/// 精确 bitmap→ranges 在轮 7 数据面落实）。
pub(crate) fn build_resume_checkpoint(
    files: &[entity::transfer_file::Model],
) -> Vec<FileCheckpoint> {
    files
        .iter()
        .map(|f| FileCheckpoint {
            file_id: f.file_id as u32,
            completed_ranges: {
                let ranges = crate::store::parse_completed_ranges(&f.completed_ranges);
                if !ranges.is_empty() {
                    ranges
                } else if f.transferred_bytes > 0 {
                    vec![(0, f.transferred_bytes as u64)]
                } else {
                    vec![]
                }
            },
        })
        .collect()
}

pub(crate) fn build_resume_manifest(files: &[entity::transfer_file::Model]) -> Vec<FileInfo> {
    files.iter().map(FileInfo::from).collect()
}

pub(crate) fn build_fetch_plan_from_files(
    files: &[entity::transfer_file::Model],
) -> AppResult<Vec<FileRange>> {
    let manifest = build_resume_manifest(files);
    let checkpoint = build_resume_checkpoint(files);
    build_fetch_plan(&manifest, &checkpoint)
}

pub(crate) fn build_fetch_plan_from_report(report: &ResumeReport) -> AppResult<Vec<FileRange>> {
    build_fetch_plan(&report.files, &report.checkpoint)
}

fn build_fetch_plan(
    manifest: &[FileInfo],
    checkpoint: &[FileCheckpoint],
) -> AppResult<Vec<FileRange>> {
    validate_checkpoint(manifest, checkpoint).map_err(|_| {
        AppError::Transfer(resume_reject_message(&ResumeRejectReason::CheckpointInvalid).into())
    })?;

    let mut plan = Vec::new();
    for file in manifest {
        let mut cursor = 0u64;
        if let Some(cp) = checkpoint.iter().find(|item| item.file_id == file.file_id) {
            for &(offset, length) in &cp.completed_ranges {
                if cursor < offset {
                    plan.push(FileRange {
                        file_id: file.file_id,
                        offset: cursor,
                        length: offset - cursor,
                    });
                }
                cursor = offset + length;
            }
        }
        if cursor < file.size {
            plan.push(FileRange {
                file_id: file.file_id,
                offset: cursor,
                length: file.size - cursor,
            });
        }
    }
    Ok(plan)
}

pub(crate) fn next_resume_epoch(local_epoch: i64, peer_epoch: i64) -> i64 {
    local_epoch.max(peer_epoch) + 1
}

pub(crate) fn build_sender_resume_state(
    files: &[entity::transfer_file::Model],
) -> HashMap<u32, (u32, u64)> {
    files
        .iter()
        .filter_map(|f| {
            let transferred = f.transferred_bytes as u64;
            if transferred == 0 {
                return None;
            }
            let file_id = f.file_id as u32;
            let file_size = f.size as u64;
            let total_chunks = calc_total_chunks(file_size);
            let chunk_size = CHUNK_SIZE as u64;

            let chunks_done = if transferred >= file_size {
                total_chunks
            } else {
                (transferred.div_ceil(chunk_size)) as u32
            };

            Some((file_id, (chunks_done, transferred)))
        })
        .collect()
}

pub(crate) fn build_resume_file_infos(
    files: &[entity::transfer_file::Model],
) -> (Vec<ResumeFileInfo>, i64) {
    let mut infos = Vec::with_capacity(files.len());
    let mut transferred_bytes: i64 = 0;
    for f in files {
        infos.push(ResumeFileInfo {
            file_id: f.file_id,
            name: f.name.clone(),
            relative_path: f.relative_path.clone(),
            size: f.size,
        });
        transferred_bytes += f.transferred_bytes;
    }
    (infos, transferred_bytes)
}

pub(crate) fn build_file_infos_and_bitmaps(
    files: &[entity::transfer_file::Model],
) -> (Vec<FileInfo>, HashMap<u32, Vec<u8>>) {
    let mut file_infos = Vec::with_capacity(files.len());
    let mut bitmaps = HashMap::with_capacity(files.len());
    for f in files {
        let fid = f.file_id as u32;
        file_infos.push(FileInfo::from(f));
        bitmaps.insert(fid, f.completed_chunks.clone());
    }
    (file_infos, bitmaps)
}

/// 从 DB 重建 PreparedFile，不做 fs 探测（让 sender 端推送时读取源文件失败再报错）
pub(crate) fn build_prepared_files_from_db(
    files: &[entity::transfer_file::Model],
) -> Vec<PreparedFile> {
    files
        .iter()
        .map(|f| PreparedFile {
            file_id: f.file_id as u32,
            name: f.name.clone(),
            relative_path: f.relative_path.clone(),
            source_id: FileSourceId(f.source_path.clone().unwrap_or_default()),
            size: f.size as u64,
            checksum: f.checksum.clone(),
            // outboard 随会话落库；旧会话（无此列）为空，发送端在 resume 前重算并回存。
            outboard: f.outboard.clone().unwrap_or_default(),
        })
        .collect()
}
