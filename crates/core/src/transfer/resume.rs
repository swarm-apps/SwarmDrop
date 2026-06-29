//! 断点续传：发送方/接收方双侧发起 + IncomingTransferRuntime 续传 helper。
//!
//! 私有 `validate_resume_session` + `ResumeContext` 集中在这里；
//! 通用辅助函数（build_*）也下沉到本文件，因为它们仅服务于 resume 流程。

use std::collections::HashMap;
use std::sync::Arc;

use entity::{TerminalReason, TransferDirection, TransferPhase};
use sea_orm::{DatabaseConnection, EntityTrait};
use swarm_p2p_core::libp2p::PeerId;
use tracing::{info, warn};
use uuid::Uuid;

use crate::host::{CoreEvent, FileSourceId};
use crate::protocol::{
    AppRequest, AppResponse, FileCheckpoint, FileInfo, FileRange, ResumePhaseReport,
    ResumeRejectReason, ResumeReport, TransferRequest, TransferResponse,
};
use crate::transfer::crypto::generate_key;
use crate::transfer::manager::{PreparedFile, ResumeFileInfo, ResumeInfo, TransferManager};
use crate::transfer::progress::{
    RuntimeTransferDirection, TransferResumedEvent, TransferResumedFileInfo,
};
use crate::transfer::sender::SendSession;
use crate::transfer::{CHUNK_SIZE, calc_total_chunks};
use crate::{AppError, AppResult};

impl TransferManager {
    pub async fn initiate_resume(&self, session_id: Uuid) -> AppResult<ResumeInfo> {
        let (session, target_peer) = load_resumable_session(&self.db, session_id).await?;
        let files = crate::database::ops::get_session_files(&self.db, session_id).await?;

        info!(
            "接收方发起探测式恢复: session={}, files={}",
            session_id,
            files.len()
        );

        let report = self
            .request_resume_probe(target_peer, session_id)
            .await?;
        if let Err(reason) = validate_resume_report(&session, &files, &report) {
            self.apply_resume_reject(&session, session_id, reason)
                .await?;
            return Err(AppError::Transfer(resume_reject_message(&reason).into()));
        }

        let key = generate_key();
        let new_epoch = next_resume_epoch(session.epoch, report.epoch);
        let fetch_plan = build_fetch_plan_from_files(&files)?;
        let total_size = session.total_size;
        let save_location = session
            .save_path
            .clone()
            .map(crate::host::CoreSaveLocation::from)
            .unwrap_or(crate::host::CoreSaveLocation::Path {
                path: String::new(),
            });
        let peer_id = session.peer_id.0.clone();
        let peer_name = session.peer_name.clone();

        let (file_infos, initial_bitmaps) = build_file_infos_and_bitmaps(&files);
        let (resume_file_infos, transferred_bytes) = build_resume_file_infos(&files);

        // 先注册新 epoch receiver，再提交 ResumeCommit，避免对端 sender 在 Ack 返回前
        // 已打开 data channel，而本端尚无 actor 导致 Hello 被拒。
        self.start_receive_session(
            new_epoch,
            session_id,
            target_peer,
            file_infos,
            total_size as u64,
            save_location,
            &key,
            initial_bitmaps,
        );
        if let Err(reason) = self
            .request_resume_commit(target_peer, session_id, new_epoch, key, fetch_plan)
            .await
        {
            if let Some(session) = self.remove_receive_session(&session_id) {
                session.cancel();
            }
            info!(
                "ResumeCommit rejected: session={}, reason={:?}",
                session_id, reason
            );
            self.apply_resume_reject(&session, session_id, reason)
                .await?;
            return Err(AppError::Transfer(resume_reject_message(&reason).into()));
        }

        self.coordinator
            .dispatch(
                session_id,
                crate::transfer::coordinator::CoordinatorInput::Network {
                    epoch: session.epoch,
                    signal: crate::transfer::coordinator::NetworkSignal::ResumeCommitted {
                        new_epoch,
                    },
                },
            )
            .await?;

        Ok(ResumeInfo {
            peer_id,
            peer_name,
            files: resume_file_infos,
            total_size,
            transferred_bytes,
        })
    }

    pub async fn initiate_resume_as_sender(&self, session_id: Uuid) -> AppResult<ResumeInfo> {
        let (session, target_peer) = load_resumable_session(&self.db, session_id).await?;
        let files = crate::database::ops::get_session_files(&self.db, session_id).await?;

        let (resume_file_infos, _) = build_resume_file_infos(&files);

        info!(
            "发送方发起探测式恢复: session={}, files={}",
            session_id,
            files.len()
        );

        let report = self
            .request_resume_probe(target_peer, session_id)
            .await?;
        if let Err(reason) = validate_resume_report(&session, &files, &report) {
            self.apply_resume_reject(&session, session_id, reason)
                .await?;
            return Err(AppError::Transfer(resume_reject_message(&reason).into()));
        }

        let key = generate_key();
        let new_epoch = next_resume_epoch(session.epoch, report.epoch);
        let fetch_plan = build_fetch_plan_from_report(&report)?;

        // 发送方必须先恢复本地 SendSession，再提交给接收方；接收方收到 commit 后可能立即拉块。
        let send_session =
            self.build_send_session_for_resume(session_id, target_peer, &files, &key);
        self.insert_send_session(session_id, new_epoch, send_session);

        if let Err(reason) = self
            .request_resume_commit(target_peer, session_id, new_epoch, key, fetch_plan.clone())
            .await
        {
            if let Some(session) = self.remove_send_session(&session_id) {
                session.cancel();
            }
            self.apply_resume_reject(&session, session_id, reason)
                .await?;
            return Err(AppError::Transfer(resume_reject_message(&reason).into()));
        }

        self.coordinator
            .dispatch(
                session_id,
                crate::transfer::coordinator::CoordinatorInput::Network {
                    epoch: session.epoch,
                    signal: crate::transfer::coordinator::NetworkSignal::ResumeCommitted {
                        new_epoch,
                    },
                },
            )
            .await?;

        self.spawn_send_data_channel(session_id, new_epoch, fetch_plan);

        let transferred_bytes: i64 = files.iter().map(|f| f.transferred_bytes).sum();

        Ok(ResumeInfo {
            peer_id: session.peer_id.0,
            peer_name: session.peer_name,
            files: resume_file_infos,
            total_size: session.total_size,
            transferred_bytes,
        })
    }
}

// ============ IncomingTransferRuntime 续传侧 helper（被 manager.rs 中 trait impl 调用） ============

impl TransferManager {
    /// 恢复探测应答：报告本端会话事实（phase/epoch/checkpoint/fingerprint/terminal）。
    pub(super) async fn handle_resume_probe_impl(
        &self,
        session_id: Uuid,
    ) -> AppResult<TransferResponse> {
        let Some(session) = crate::database::ops::find_session(&self.db, session_id).await? else {
            return Ok(TransferResponse::ResumeStateReport {
                session_id,
                report: ResumeReport {
                    phase: ResumePhaseReport::NotFound,
                    epoch: 0,
                    files: vec![],
                    checkpoint: vec![],
                    source_fingerprint: None,
                    terminal: false,
                    terminal_reason: None,
                },
            });
        };
        let files = crate::database::ops::get_session_files(&self.db, session_id).await?;
        Ok(TransferResponse::ResumeStateReport {
            session_id,
            report: ResumeReport {
                phase: map_resume_phase(&session.phase),
                epoch: session.epoch,
                files: build_resume_manifest(&files),
                checkpoint: build_resume_checkpoint(&files),
                source_fingerprint: session.source_fingerprint,
                terminal: matches!(session.phase, entity::TransferPhase::Terminal),
                terminal_reason: session.terminal_reason,
            },
        })
    }

    /// 恢复提交应答：校验后经 Coordinator 转 active(new_epoch)，完成 epoch 递增。
    /// 注：actor 重建 + 续传搬运在轮 7（数据面）接入；此处先做状态转换。
    pub(super) async fn handle_resume_commit_impl(
        &self,
        peer_id: PeerId,
        session_id: Uuid,
        new_epoch: i64,
        key: [u8; 32],
        fetch_plan: Vec<FileRange>,
    ) -> AppResult<TransferResponse> {
        let Some(session) = crate::database::ops::find_session(&self.db, session_id).await? else {
            return Ok(TransferResponse::ResumeAck {
                session_id,
                new_epoch,
                accepted: false,
                reason: Some(ResumeRejectReason::SessionNotFound),
            });
        };
        let files = crate::database::ops::get_session_files(&self.db, session_id).await?;
        if let Err(reason) = validate_resume_commit(&session, &files, new_epoch, &fetch_plan) {
            return Ok(TransferResponse::ResumeAck {
                session_id,
                new_epoch,
                accepted: false,
                reason: Some(reason),
            });
        }

        let transitioned = self
            .coordinator
            .dispatch(
                session_id,
                crate::transfer::coordinator::CoordinatorInput::Network {
                    epoch: session.epoch,
                    signal: crate::transfer::coordinator::NetworkSignal::ResumeCommitted {
                        new_epoch,
                    },
                },
            )
            .await?;
        if transitioned.is_none() {
            // 走到这里说明本端非 suspended（多为对端探测后我方仍 Active 未感知中断），
            // reduce 拒绝转换。回 PeerUnavailable（发起方 apply_resume_reject no-op，保持
            // 可重试）而非 CheckpointInvalid（会被发起方归入 FatalError 永久打死会话）。
            return Ok(TransferResponse::ResumeAck {
                session_id,
                new_epoch,
                accepted: false,
                reason: Some(ResumeRejectReason::PeerUnavailable),
            });
        }

        self.start_local_resume_actor(peer_id, &session, &files, &key, new_epoch, fetch_plan);
        let _ = self
            .event_bus
            .publish(CoreEvent::TransferResumed {
                event: build_transfer_resumed_event(
                    &session,
                    &files,
                    match session.direction {
                        TransferDirection::Send => RuntimeTransferDirection::Send,
                        TransferDirection::Receive => RuntimeTransferDirection::Receive,
                    },
                ),
            })
            .await;

        Ok(TransferResponse::ResumeAck {
            session_id,
            new_epoch,
            accepted: true,
            reason: None,
        })
    }
}

impl TransferManager {
    async fn request_resume_probe(
        &self,
        target_peer: PeerId,
        session_id: Uuid,
    ) -> AppResult<ResumeReport> {
        let response = self
            .client
            .send_request(
                target_peer,
                AppRequest::Transfer(TransferRequest::ResumeProbe { session_id }),
            )
            .await
            .map_err(|e| AppError::Transfer(format!("ResumeProbe 发送失败: {e}")))?;

        match response {
            AppResponse::Transfer(TransferResponse::ResumeStateReport {
                session_id: response_id,
                report,
            }) if response_id == session_id => Ok(report),
            other => Err(AppError::Transfer(format!(
                "ResumeProbe 收到意外响应: {other:?}"
            ))),
        }
    }

    async fn request_resume_commit(
        &self,
        target_peer: PeerId,
        session_id: Uuid,
        new_epoch: i64,
        key: [u8; 32],
        fetch_plan: Vec<FileRange>,
    ) -> Result<(), ResumeRejectReason> {
        let response = self
            .client
            .send_request(
                target_peer,
                AppRequest::Transfer(TransferRequest::ResumeCommit {
                    session_id,
                    new_epoch,
                    key,
                    fetch_plan,
                }),
            )
            .await
            .map_err(|e| {
                warn!("ResumeCommit 发送失败: session={}, {}", session_id, e);
                ResumeRejectReason::PeerUnavailable
            })?;

        match response {
            AppResponse::Transfer(TransferResponse::ResumeAck {
                session_id: response_id,
                new_epoch: ack_epoch,
                accepted: true,
                ..
            }) if response_id == session_id && ack_epoch == new_epoch => Ok(()),
            AppResponse::Transfer(TransferResponse::ResumeAck { accepted: true, .. }) => {
                Err(ResumeRejectReason::CheckpointInvalid)
            }
            AppResponse::Transfer(TransferResponse::ResumeAck {
                accepted: false,
                reason,
                ..
            }) => Err(reason.unwrap_or(ResumeRejectReason::FatalError)),
            other => {
                warn!("ResumeCommit 收到意外响应: {:?}", other);
                Err(ResumeRejectReason::FatalError)
            }
        }
    }

    async fn apply_resume_reject(
        &self,
        session: &entity::transfer_session::Model,
        session_id: Uuid,
        reason: ResumeRejectReason,
    ) -> AppResult<()> {
        use crate::transfer::coordinator::{ActorReport, CoordinatorInput, UserCommand};

        match reason {
            ResumeRejectReason::Cancelled => {
                self.coordinator
                    .dispatch(session_id, CoordinatorInput::User(UserCommand::Cancel))
                    .await?;
            }
            ResumeRejectReason::FatalError
            | ResumeRejectReason::SourceModified
            | ResumeRejectReason::CheckpointInvalid
            | ResumeRejectReason::SessionNotFound => {
                self.coordinator
                    .dispatch(
                        session_id,
                        CoordinatorInput::Actor {
                            epoch: session.epoch,
                            report: ActorReport::FatalError(
                                resume_reject_message(&reason).to_string(),
                            ),
                        },
                    )
                    .await?;
            }
            ResumeRejectReason::PeerUnavailable => {}
        }
        Ok(())
    }

    fn build_send_session_for_resume(
        &self,
        session_id: Uuid,
        peer_id: PeerId,
        files: &[entity::transfer_file::Model],
        key: &[u8; 32],
    ) -> Arc<SendSession> {
        let prepared_files = build_prepared_files_from_db(files);
        let resume_state = build_sender_resume_state(files);
        Arc::new(SendSession::new_with_resume(
            session_id,
            peer_id,
            prepared_files,
            key,
            self.file_access.clone(),
            self.event_bus.clone(),
            &resume_state,
        ))
    }

    fn start_local_resume_actor(
        &self,
        peer_id: PeerId,
        session: &entity::transfer_session::Model,
        files: &[entity::transfer_file::Model],
        key: &[u8; 32],
        new_epoch: i64,
        fetch_plan: Vec<FileRange>,
    ) {
        match session.direction {
            TransferDirection::Send => {
                let send_session =
                    self.build_send_session_for_resume(session.session_id, peer_id, files, key);
                self.insert_send_session(session.session_id, new_epoch, send_session);
                self.spawn_send_data_channel(session.session_id, new_epoch, fetch_plan);
            }
            TransferDirection::Receive => {
                let (file_infos, initial_bitmaps) = build_file_infos_and_bitmaps(files);
                let save_location = session
                    .save_path
                    .clone()
                    .map(crate::host::CoreSaveLocation::from)
                    .unwrap_or(crate::host::CoreSaveLocation::Path {
                        path: String::new(),
                    });
                self.start_receive_from_offer(
                    new_epoch,
                    session.session_id,
                    peer_id,
                    file_infos,
                    session.total_size as u64,
                    save_location,
                    key,
                    initial_bitmaps,
                );
            }
        }
    }
}

// ============ 续传共用的私有 helper ============

/// entity phase → 恢复探测报告的简化 phase。
fn map_resume_phase(phase: &entity::TransferPhase) -> ResumePhaseReport {
    match phase {
        entity::TransferPhase::Active
        | entity::TransferPhase::Offered
        | entity::TransferPhase::WaitingAccept => ResumePhaseReport::Active,
        entity::TransferPhase::Suspended => ResumePhaseReport::Suspended,
        entity::TransferPhase::Terminal => ResumePhaseReport::Terminal,
    }
}

/// 从文件记录构造 checkpoint（过渡近似：transferred_bytes 作单个连续 range；
/// 精确 bitmap→ranges 在轮 7 数据面落实）。
fn build_resume_checkpoint(files: &[entity::transfer_file::Model]) -> Vec<FileCheckpoint> {
    files
        .iter()
        .map(|f| FileCheckpoint {
            file_id: f.file_id as u32,
            completed_ranges: {
                let ranges = crate::database::ops::parse_completed_ranges(&f.completed_ranges);
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

fn build_resume_manifest(files: &[entity::transfer_file::Model]) -> Vec<FileInfo> {
    files
        .iter()
        .map(|f| FileInfo {
            file_id: f.file_id as u32,
            name: f.name.clone(),
            relative_path: f.relative_path.clone(),
            size: f.size as u64,
            checksum: f.checksum.clone(),
        })
        .collect()
}

fn validate_resume_report(
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

fn validate_resume_commit(
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

fn validate_checkpoint(
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

fn build_fetch_plan_from_files(
    files: &[entity::transfer_file::Model],
) -> AppResult<Vec<FileRange>> {
    let manifest = build_resume_manifest(files);
    let checkpoint = build_resume_checkpoint(files);
    build_fetch_plan(&manifest, &checkpoint)
}

fn build_fetch_plan_from_report(report: &ResumeReport) -> AppResult<Vec<FileRange>> {
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

fn next_resume_epoch(local_epoch: i64, peer_epoch: i64) -> i64 {
    local_epoch.max(peer_epoch) + 1
}

fn resume_reject_message(reason: &ResumeRejectReason) -> &'static str {
    match reason {
        ResumeRejectReason::Cancelled => "对端已取消传输，无法恢复",
        ResumeRejectReason::FatalError => "对端报告不可恢复错误",
        ResumeRejectReason::SourceModified => "源文件或传输清单已变更，无法恢复",
        ResumeRejectReason::CheckpointInvalid => "断点续传进度无效，无法恢复",
        ResumeRejectReason::PeerUnavailable => "对端不可用，请稍后再试",
        ResumeRejectReason::SessionNotFound => "对端找不到对应会话",
    }
}

// ============ 断点续传辅助函数 ============

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

pub(crate) fn parse_peer_id(s: &str) -> AppResult<PeerId> {
    s.parse()
        .map_err(|_| AppError::Transfer(format!("无效的 PeerId: {s}")))
}

pub(crate) async fn load_resumable_session(
    db: &DatabaseConnection,
    session_id: Uuid,
) -> AppResult<(entity::transfer_session::Model, PeerId)> {
    let session = entity::TransferSession::find_by_id(session_id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::Transfer("会话不存在".into()))?;

    if !matches!(session.phase, TransferPhase::Suspended) || !session.recoverable {
        return Err(AppError::Transfer(format!(
            "会话状态不支持恢复: phase={:?}, recoverable={}",
            session.phase, session.recoverable
        )));
    }

    let target_peer = parse_peer_id(&session.peer_id.0)?;
    Ok((session, target_peer))
}

fn build_transfer_resumed_event(
    session: &entity::transfer_session::Model,
    files: &[entity::transfer_file::Model],
    direction: RuntimeTransferDirection,
) -> TransferResumedEvent {
    let resumed_files = files
        .iter()
        .map(|f| TransferResumedFileInfo {
            file_id: f.file_id as u32,
            name: f.name.clone(),
            relative_path: f.relative_path.clone(),
            size: f.size as u64,
            is_directory: false,
        })
        .collect();

    TransferResumedEvent {
        session_id: session.session_id,
        direction,
        peer_id: session.peer_id.0.clone(),
        peer_name: session.peer_name.clone(),
        files: resumed_files,
        total_size: session.total_size as u64,
    }
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
        file_infos.push(FileInfo {
            file_id: fid,
            name: f.name.clone(),
            relative_path: f.relative_path.clone(),
            size: f.size as u64,
            checksum: f.checksum.clone(),
        });
        bitmaps.insert(fid, f.completed_chunks.clone());
    }
    (file_infos, bitmaps)
}

/// 从 DB 重建 PreparedFile，不做 fs 探测（让 sender 端 chunk_request 失败时报错）
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
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(session_id: Uuid) -> entity::transfer_session::Model {
        entity::transfer_session::Model {
            session_id,
            direction: entity::TransferDirection::Send,
            peer_id: entity::PeerId("peer-123".to_string()),
            peer_name: "测试设备".to_string(),
            total_size: 42,
            transferred_bytes: 7,
            status: entity::SessionStatus::Paused,
            phase: entity::TransferPhase::Suspended,
            suspended_reason: Some(entity::SuspendedReason::LocalPaused),
            terminal_reason: None,
            epoch: 1,
            recoverable: true,
            source_fingerprint: None,
            started_at: 1,
            updated_at: 2,
            finished_at: None,
            error_message: None,
            policy_action: None,
            policy_reason: None,
            save_path: None,
        }
    }

    fn file(session_id: Uuid) -> entity::transfer_file::Model {
        entity::transfer_file::Model {
            id: 1,
            session_id,
            file_id: 7,
            name: "resume.txt".to_string(),
            relative_path: "nested/resume.txt".to_string(),
            size: 42,
            checksum: "checksum".to_string(),
            status: entity::FileStatus::Pending,
            transferred_bytes: 7,
            total_chunks: 1,
            completed_chunks: vec![1],
            completed_ranges: serde_json::to_string(&vec![(0u64, 7u64)]).unwrap(),
            source_path: Some("/tmp/resume.txt".to_string()),
        }
    }

    fn report_from(files: &[entity::transfer_file::Model]) -> ResumeReport {
        ResumeReport {
            phase: ResumePhaseReport::Suspended,
            epoch: 1,
            files: build_resume_manifest(files),
            checkpoint: build_resume_checkpoint(files),
            source_fingerprint: None,
            terminal: false,
            terminal_reason: None,
        }
    }

    #[test]
    fn resumed_event_should_mark_sender_active_when_receiver_initiates_resume() {
        let session_id = Uuid::new_v4();
        let session = session(session_id);
        let files = vec![file(session_id)];

        let event = build_transfer_resumed_event(&session, &files, RuntimeTransferDirection::Send);

        assert_eq!(event.session_id, session_id);
        assert_eq!(event.direction, RuntimeTransferDirection::Send);
        assert_eq!(event.peer_id, "peer-123");
        assert_eq!(event.peer_name, "测试设备");
        assert_eq!(event.total_size, 42);
        assert_eq!(event.files.len(), 1);
        assert_eq!(event.files[0].file_id, 7);
        assert_eq!(event.files[0].relative_path, "nested/resume.txt");
    }

    #[test]
    fn resumed_event_should_mark_receiver_active_when_sender_initiates_resume() {
        let session_id = Uuid::new_v4();
        let session = session(session_id);
        let files = vec![file(session_id)];

        let event =
            build_transfer_resumed_event(&session, &files, RuntimeTransferDirection::Receive);

        assert_eq!(event.direction, RuntimeTransferDirection::Receive);
    }

    #[test]
    fn validate_resume_report_accepts_matching_suspended_manifest() {
        let session_id = Uuid::new_v4();
        let session = session(session_id);
        let files = vec![file(session_id)];
        let report = report_from(&files);

        validate_resume_report(&session, &files, &report).unwrap();
    }

    #[test]
    fn validate_resume_report_maps_terminal_cancelled() {
        let session_id = Uuid::new_v4();
        let session = session(session_id);
        let files = vec![file(session_id)];
        let mut report = report_from(&files);
        report.phase = ResumePhaseReport::Terminal;
        report.terminal = true;
        report.terminal_reason = Some(TerminalReason::Cancelled);

        assert_eq!(
            validate_resume_report(&session, &files, &report),
            Err(ResumeRejectReason::Cancelled)
        );
    }

    #[test]
    fn validate_resume_report_rejects_source_manifest_change() {
        let session_id = Uuid::new_v4();
        let session = session(session_id);
        let files = vec![file(session_id)];
        let mut report = report_from(&files);
        report.files[0].checksum = "changed".to_string();

        assert_eq!(
            validate_resume_report(&session, &files, &report),
            Err(ResumeRejectReason::SourceModified)
        );
    }

    #[test]
    fn validate_resume_report_rejects_source_fingerprint_change() {
        let session_id = Uuid::new_v4();
        let mut session = session(session_id);
        session.source_fingerprint = Some("local".to_string());
        let files = vec![file(session_id)];
        let mut report = report_from(&files);
        report.source_fingerprint = Some("remote".to_string());

        assert_eq!(
            validate_resume_report(&session, &files, &report),
            Err(ResumeRejectReason::SourceModified)
        );
    }

    #[test]
    fn validate_resume_report_rejects_checkpoint_out_of_bounds() {
        let session_id = Uuid::new_v4();
        let session = session(session_id);
        let files = vec![file(session_id)];
        let mut report = report_from(&files);
        report.checkpoint[0].completed_ranges = vec![(0, 43)];

        assert_eq!(
            validate_resume_report(&session, &files, &report),
            Err(ResumeRejectReason::CheckpointInvalid)
        );
    }

    #[test]
    fn validate_resume_commit_rejects_old_epoch() {
        let session_id = Uuid::new_v4();
        let session = session(session_id);
        let files = vec![file(session_id)];
        let fetch_plan = build_fetch_plan_from_files(&files).unwrap();

        assert_eq!(
            validate_resume_commit(&session, &files, 1, &fetch_plan),
            Err(ResumeRejectReason::CheckpointInvalid)
        );
    }

    #[test]
    fn build_fetch_plan_skips_completed_prefix() {
        let session_id = Uuid::new_v4();
        let files = vec![file(session_id)];

        assert_eq!(
            build_fetch_plan_from_files(&files).unwrap(),
            vec![FileRange {
                file_id: 7,
                offset: 7,
                length: 35,
            }]
        );
    }
}
