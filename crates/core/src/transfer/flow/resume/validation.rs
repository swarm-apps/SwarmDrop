//! 断点续传纯校验 + reject reason 文案（无 `self` / `TransferManager` 依赖）。
//!
//! 与 [`super::plan`] 互相引用（本模块用其 `build_resume_manifest`，plan 用本模块
//! `validate_checkpoint` / `resume_reject_message`），同 crate 双向模块引用无碍。

use entity::{TerminalReason, TransferPhase};

use crate::protocol::{
    FileCheckpoint, FileInfo, FileRange, ResumePhaseReport, ResumeRejectReason, ResumeReport,
};

use super::plan::build_resume_manifest;

/// entity phase → 恢复探测报告的简化 phase。
pub(crate) fn map_resume_phase(phase: &entity::TransferPhase) -> ResumePhaseReport {
    match phase {
        entity::TransferPhase::Active
        | entity::TransferPhase::Offered
        | entity::TransferPhase::WaitingAccept => ResumePhaseReport::Active,
        entity::TransferPhase::Suspended => ResumePhaseReport::Suspended,
        entity::TransferPhase::Terminal => ResumePhaseReport::Terminal,
    }
}

pub(crate) fn validate_resume_report(
    session: &entity::transfer_session::Model,
    local_files: &[entity::transfer_file::Model],
    report: &ResumeReport,
) -> Result<(), ResumeRejectReason> {
    match report.phase {
        ResumePhaseReport::NotFound => return Err(ResumeRejectReason::SessionNotFound),
        ResumePhaseReport::Terminal => {
            return match report.terminal_reason {
                Some(TerminalReason::Cancelled) => Err(ResumeRejectReason::Cancelled),
                _ => Err(ResumeRejectReason::FatalError),
            };
        }
        // 对端仍在传输中（Active/Offered/WaitingAccept），尚未感知中断、无法接受
        // ResumeCommit（应答侧 reduce 受 is_suspended 守卫）。视为暂时不可用而非致命：
        // apply_resume_reject 对 PeerUnavailable no-op，保持本端 suspended/recoverable，
        // 待对端也转入 suspended 后重试即可——避免把可恢复会话误打成永久 FatalError。
        ResumePhaseReport::Active => return Err(ResumeRejectReason::PeerUnavailable),
        ResumePhaseReport::Suspended => {}
    }

    let local_manifest = build_resume_manifest(local_files);
    validate_manifest_match(&local_manifest, &report.files)?;
    validate_checkpoint(&report.files, &report.checkpoint)?;

    if let (Some(local), Some(remote)) = (
        session.source_fingerprint.as_ref(),
        report.source_fingerprint.as_ref(),
    ) && local != remote
    {
        return Err(ResumeRejectReason::SourceModified);
    }

    Ok(())
}

pub(crate) fn validate_resume_commit(
    session: &entity::transfer_session::Model,
    files: &[entity::transfer_file::Model],
    new_epoch: i64,
    fetch_plan: &[FileRange],
) -> Result<(), ResumeRejectReason> {
    if matches!(session.terminal_reason, Some(TerminalReason::Cancelled)) {
        return Err(ResumeRejectReason::Cancelled);
    }
    if matches!(session.phase, TransferPhase::Terminal) || !session.recoverable {
        return Err(ResumeRejectReason::FatalError);
    }
    if new_epoch <= session.epoch {
        return Err(ResumeRejectReason::CheckpointInvalid);
    }

    let manifest = build_resume_manifest(files);
    validate_fetch_plan(&manifest, fetch_plan)
}

fn validate_manifest_match(
    local: &[FileInfo],
    remote: &[FileInfo],
) -> Result<(), ResumeRejectReason> {
    if local.len() != remote.len() {
        return Err(ResumeRejectReason::SourceModified);
    }
    for file in local {
        let Some(peer_file) = remote.iter().find(|f| f.file_id == file.file_id) else {
            return Err(ResumeRejectReason::SourceModified);
        };
        if peer_file.size != file.size
            || peer_file.checksum != file.checksum
            || peer_file.relative_path != file.relative_path
        {
            return Err(ResumeRejectReason::SourceModified);
        }
    }
    Ok(())
}

pub(crate) fn validate_checkpoint(
    manifest: &[FileInfo],
    checkpoint: &[FileCheckpoint],
) -> Result<(), ResumeRejectReason> {
    for item in checkpoint {
        let Some(file) = manifest.iter().find(|f| f.file_id == item.file_id) else {
            return Err(ResumeRejectReason::CheckpointInvalid);
        };
        let mut previous_end = 0u64;
        for &(offset, length) in &item.completed_ranges {
            let end = offset
                .checked_add(length)
                .ok_or(ResumeRejectReason::CheckpointInvalid)?;
            if length == 0 || offset < previous_end || end > file.size {
                return Err(ResumeRejectReason::CheckpointInvalid);
            }
            previous_end = end;
        }
    }
    Ok(())
}

fn validate_fetch_plan(
    manifest: &[FileInfo],
    fetch_plan: &[FileRange],
) -> Result<(), ResumeRejectReason> {
    for range in fetch_plan {
        let Some(file) = manifest.iter().find(|f| f.file_id == range.file_id) else {
            return Err(ResumeRejectReason::CheckpointInvalid);
        };
        let end = range
            .offset
            .checked_add(range.length)
            .ok_or(ResumeRejectReason::CheckpointInvalid)?;
        if range.length == 0 || end > file.size {
            return Err(ResumeRejectReason::CheckpointInvalid);
        }
    }
    Ok(())
}

pub(crate) fn resume_reject_message(reason: &ResumeRejectReason) -> &'static str {
    match reason {
        ResumeRejectReason::Cancelled => "对端已取消传输，无法恢复",
        ResumeRejectReason::FatalError => "对端报告不可恢复错误",
        ResumeRejectReason::SourceModified => "源文件或传输清单已变更，无法恢复",
        ResumeRejectReason::CheckpointInvalid => "断点续传进度无效，无法恢复",
        ResumeRejectReason::PeerUnavailable => "对端不可用，请稍后再试",
        ResumeRejectReason::SessionNotFound => "对端找不到对应会话",
    }
}
