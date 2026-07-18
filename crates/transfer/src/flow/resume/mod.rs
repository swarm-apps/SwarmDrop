//! 断点续传编排：双侧发起模板 `initiate_resume` + IncomingTransferRuntime 续传 helper。
//!
//! 本文件保留依赖 `self` / `TransferManager` 的编排逻辑；纯函数下沉到两个子模块：
//! - [`validation`] —— 探测报告 / commit / checkpoint 校验 + reject reason 文案
//! - [`plan`] —— manifest / checkpoint / fetch_plan / 续传 state 的派生构造

use std::sync::Arc;

use entity::{TransferDirection, TransferPhase};
use swarmdrop_net::NodeId;
use tracing::{info, warn};
use uuid::Uuid;

use crate::actor::sender::SenderActor;
use crate::events::TransferEvent;
use crate::manager::{ResumeInfo, TransferManager};
use crate::progress::{RuntimeTransferDirection, TransferResumedEvent, TransferResumedFileInfo};
use crate::protocol::{
    FileRange, ResumePhaseReport, ResumeRejectReason, ResumeReport, TRANSFER_CTRL, TransferRequest,
    TransferResponse,
};
use crate::store::SessionStore;
use crate::{AppError, AppResult};

mod plan;
mod validation;

use plan::{
    build_fetch_plan_from_files, build_fetch_plan_from_report, build_file_infos_and_bitmaps,
    build_prepared_files_from_db, build_resume_checkpoint, build_resume_file_infos,
    build_resume_manifest, build_sender_resume_state, next_resume_epoch,
};
use validation::{
    map_resume_phase, resume_reject_message, validate_resume_commit, validate_resume_report,
};

impl TransferManager {
    /// 断点续传统一入口（发送方 / 接收方发起共用模板）。
    ///
    /// 公共流程：load → probe → validate（失败 apply_resume_reject 后返回）→ key/epoch →
    /// 构造 fetch_plan → 注册新 epoch actor → commit（失败回滚）→ dispatch(ResumeCommitted)
    /// →（仅 Send）spawn 数据面 → 返回 ResumeInfo。方向差异仅 5 点，全由 `session.direction`
    /// 派生（见各 ▼ 注释）。
    ///
    /// **安全序**（两 agent 审查确认，勿动）：
    /// - 注册 actor 必须在 `request_resume_commit` **之前**——否则对端 sender 在 Ack 返回前
    ///   打开 data channel 时本端尚无 actor → Hello 被拒；commit 失败再 `rollback_resume_actor`。
    /// - `spawn_send_data_channel` 必须在 `dispatch(ResumeCommitted)` **之后**（dispatch 把
    ///   phase 转 active）。
    /// - `dispatch` 用**旧** `session.epoch`，actor 注册 / spawn 用 `new_epoch`，勿混。
    pub async fn initiate_resume(&self, session_id: Uuid) -> AppResult<ResumeInfo> {
        let (session, target_peer) =
            load_resumable_session(self.store.as_ref(), session_id).await?;
        let files = self.store.get_session_files(session_id).await?;

        // ▼ D0 日志文案
        let role = match session.direction {
            TransferDirection::Send => "发送方",
            TransferDirection::Receive => "接收方",
        };
        info!(
            "{role}发起探测式恢复: session={}, files={}",
            session_id,
            files.len()
        );

        let report = self.request_resume_probe(target_peer, session_id).await?;
        if let Err(reason) = validate_resume_report(&session, &files, &report) {
            self.apply_resume_reject(&session, session_id, reason)
                .await?;
            return Err(AppError::Transfer(resume_reject_message(&reason).into()));
        }

        let new_epoch = next_resume_epoch(session.epoch, report.epoch);

        // ▼ A fetch_plan 来源：接收方用本端 DB 推算，发送方用对端 report 推算
        let fetch_plan = match session.direction {
            TransferDirection::Receive => build_fetch_plan_from_files(&files)?,
            TransferDirection::Send => build_fetch_plan_from_report(&report)?,
        };

        // ▼ B 注册新 epoch actor（commit 前，不含 spawn）
        self.register_resume_actor(&session, &files, new_epoch, target_peer);

        // ▼ D 仅 Send 在 dispatch 后 spawn 复用 fetch_plan；Receive 无 spawn，故把
        // fetch_plan 直接 move 进 commit（不克隆），只有 Send 才提前克隆一份留给 spawn。
        let send_plan =
            matches!(session.direction, TransferDirection::Send).then(|| fetch_plan.clone());

        if let Err(reason) = self
            .request_resume_commit(target_peer, session_id, new_epoch, fetch_plan)
            .await
        {
            // ▼ C 回滚：按 new_epoch 守卫 remove + cancel（与 teardown 路径一致），再 reject
            self.rollback_resume_actor(&session, session_id, new_epoch);
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
                crate::coordinator::CoordinatorInput::Network {
                    epoch: session.epoch,
                    signal: crate::coordinator::NetworkSignal::ResumeCommitted { new_epoch },
                },
            )
            .await?;

        // ▼ D 激活后 spawn（仅 Send，必须在 dispatch 之后）
        if let Some(send_plan) = send_plan {
            self.spawn_send_data_channel(session_id, new_epoch, send_plan);
        }

        // transferred_bytes 两端恒等（均为 sum(f.transferred_bytes)）：统一取元组第二元素。
        let (resume_file_infos, transferred_bytes) = build_resume_file_infos(&files);
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
    pub(crate) async fn handle_resume_probe_impl(
        &self,
        session_id: Uuid,
    ) -> AppResult<TransferResponse> {
        let Some(session) = self.store.find_session(session_id).await? else {
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
        let files = self.store.get_session_files(session_id).await?;
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
    pub(crate) async fn handle_resume_commit_impl(
        &self,
        peer_id: NodeId,
        session_id: Uuid,
        new_epoch: i64,
        fetch_plan: Vec<FileRange>,
    ) -> AppResult<TransferResponse> {
        let Some(session) = self.store.find_session(session_id).await? else {
            return Ok(TransferResponse::ResumeAck {
                session_id,
                new_epoch,
                accepted: false,
                reason: Some(ResumeRejectReason::SessionNotFound),
            });
        };
        let files = self.store.get_session_files(session_id).await?;
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
                crate::coordinator::CoordinatorInput::Network {
                    epoch: session.epoch,
                    signal: crate::coordinator::NetworkSignal::ResumeCommitted { new_epoch },
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

        self.start_local_resume_actor(peer_id, &session, &files, new_epoch, fetch_plan);
        let _ = self
            .events
            .emit(TransferEvent::TransferResumed {
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
        target_peer: NodeId,
        session_id: Uuid,
    ) -> AppResult<ResumeReport> {
        let response = TRANSFER_CTRL
            .call(
                &self.endpoint,
                target_peer,
                &TransferRequest::ResumeProbe { session_id },
            )
            .await
            .map_err(|e| AppError::Transfer(format!("ResumeProbe 发送失败: {e}")))?;

        match response {
            TransferResponse::ResumeStateReport {
                session_id: response_id,
                report,
            } if response_id == session_id => Ok(report),
            other => Err(AppError::Transfer(format!(
                "ResumeProbe 收到意外响应: {other:?}"
            ))),
        }
    }

    async fn request_resume_commit(
        &self,
        target_peer: NodeId,
        session_id: Uuid,
        new_epoch: i64,
        fetch_plan: Vec<FileRange>,
    ) -> Result<(), ResumeRejectReason> {
        let response = TRANSFER_CTRL
            .call(
                &self.endpoint,
                target_peer,
                &TransferRequest::ResumeCommit {
                    session_id,
                    new_epoch,
                    fetch_plan,
                },
            )
            .await
            .map_err(|e| {
                warn!("ResumeCommit 发送失败: session={}, {}", session_id, e);
                ResumeRejectReason::PeerUnavailable
            })?;

        match response {
            TransferResponse::ResumeAck {
                session_id: response_id,
                new_epoch: ack_epoch,
                accepted: true,
                ..
            } if response_id == session_id && ack_epoch == new_epoch => Ok(()),
            TransferResponse::ResumeAck { accepted: true, .. } => {
                Err(ResumeRejectReason::CheckpointInvalid)
            }
            TransferResponse::ResumeAck {
                accepted: false,
                reason,
                ..
            } => Err(reason.unwrap_or(ResumeRejectReason::FatalError)),
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
        use crate::coordinator::{ActorReport, CoordinatorInput, UserCommand};

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

    fn build_sender_actor_for_resume(
        &self,
        session_id: Uuid,
        peer_id: NodeId,
        files: &[entity::transfer_file::Model],
    ) -> Arc<SenderActor> {
        let prepared_files = build_prepared_files_from_db(files);
        let resume_state = build_sender_resume_state(files);
        Arc::new(SenderActor::new_with_resume(
            session_id,
            peer_id,
            prepared_files,
            self.file_access.clone(),
            self.events.clone(),
            &resume_state,
        ))
    }

    /// 按方向重建并注册新 epoch actor（**仅构造 + insert，不 spawn**）。
    ///
    /// 主动侧 [`initiate_resume`](Self::initiate_resume)（commit 前）与被动应答侧
    /// [`start_local_resume_actor`](Self::start_local_resume_actor)（transition 后）共用；
    /// `spawn_send_data_channel` 在两侧都作为独立的「激活后」步骤，满足「spawn 在 active
    /// 之后」时序——绝不塞进本 helper，否则主动侧会在 commit/dispatch 前就推送数据面。
    fn register_resume_actor(
        &self,
        session: &entity::transfer_session::Model,
        files: &[entity::transfer_file::Model],
        new_epoch: i64,
        peer_id: NodeId,
    ) {
        match session.direction {
            TransferDirection::Send => {
                let send_actor =
                    self.build_sender_actor_for_resume(session.session_id, peer_id, files);
                self.insert_send_actor(session.session_id, new_epoch, send_actor);
            }
            TransferDirection::Receive => {
                let (file_infos, initial_bitmaps) = build_file_infos_and_bitmaps(files);
                let save_location = build_save_location(session);
                self.start_receive_actor(
                    new_epoch,
                    session.session_id,
                    peer_id,
                    file_infos,
                    session.total_size as u64,
                    save_location,
                    initial_bitmaps,
                );
            }
        }
    }

    /// commit 失败时回滚刚注册的新 epoch actor（按方向 + new_epoch 守卫 remove + cancel）。
    ///
    /// 用 `remove_*_if_epoch(new_epoch)` 而非无条件 remove：register→commit(await)→rollback
    /// 之间若有更高 epoch 的并发 resume 抢注，这里不会误删它（与 teardown 路径同纪律）。
    fn rollback_resume_actor(
        &self,
        session: &entity::transfer_session::Model,
        session_id: Uuid,
        new_epoch: i64,
    ) {
        match session.direction {
            TransferDirection::Send => {
                if let Some(actor) = self.actors.remove_send_if_epoch(&session_id, new_epoch) {
                    actor.cancel();
                }
            }
            TransferDirection::Receive => {
                if let Some(actor) = self.actors.remove_receive_if_epoch(&session_id, new_epoch) {
                    actor.cancel();
                }
            }
        }
    }

    /// 被动应答侧（`handle_resume_commit_impl` transition 成功后）重建 actor。
    /// transition 已先行，故注册后立即 spawn（仅 Send）满足「spawn 在 active 之后」。
    fn start_local_resume_actor(
        &self,
        peer_id: NodeId,
        session: &entity::transfer_session::Model,
        files: &[entity::transfer_file::Model],
        new_epoch: i64,
        fetch_plan: Vec<FileRange>,
    ) {
        self.register_resume_actor(session, files, new_epoch, peer_id);
        if matches!(session.direction, TransferDirection::Send) {
            self.spawn_send_data_channel(session.session_id, new_epoch, fetch_plan);
        }
    }
}

// ============ 断点续传辅助函数 ============

pub(crate) fn parse_peer_id(s: &str) -> AppResult<NodeId> {
    s.parse()
        .map_err(|_| AppError::Transfer(format!("无效的 NodeId: {s}")))
}

/// `session.save_path` → `CoreSaveLocation`，缺省回退空路径（host 自行兜底语义）。
fn build_save_location(session: &entity::transfer_session::Model) -> crate::host::CoreSaveLocation {
    session
        .save_path
        .clone()
        .map(crate::host::CoreSaveLocation::from)
        .unwrap_or(crate::host::CoreSaveLocation::Path {
            path: String::new(),
        })
}

async fn load_resumable_session(
    store: &dyn SessionStore,
    session_id: Uuid,
) -> AppResult<(entity::transfer_session::Model, NodeId)> {
    // 收编：不再直连 ORM（`find_by_id`），改经持久化端口 find_session；恢复校验
    // （phase=Suspended + recoverable）与 peer 解析仍是 transfer 域逻辑，留在此处。
    let session = store
        .find_session(session_id)
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

#[cfg(test)]
mod tests {
    use entity::TerminalReason;

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
            origin: None,
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
            local_path: None,
            local_dir: None,
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
