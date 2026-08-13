//! 断点续传编排：双侧发起模板 `initiate_resume` + IncomingTransferRuntime 续传 helper。
//!
//! 本文件保留依赖 `self` / `TransferManager` 的编排逻辑；纯函数下沉到两个子模块：
//! - [`validation`] —— 探测报告 / commit / checkpoint 校验 + reject reason 文案
//! - [`plan`] —— manifest / checkpoint / fetch_plan / 续传 state 的派生构造

use std::collections::HashMap;
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
    build_resume_manifest, build_sender_resume_state_from_plan, next_resume_epoch,
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
        // `mut`：发送侧的进度基线算出来后要**就地回填**这份快照——下面 dispatch 会 emit
        // 一份 projection、末尾还要拿它算 `ResumeInfo.transferred_bytes`，两处都读它。
        let mut files = self.store.get_session_files(session_id).await?;

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

        // ▼ A2 先算进度基线并落地（**必须在 dispatch 之前**，见
        // `apply_sender_resume_baseline`）。它同时回写 DB 与就地回填 `files`，
        // 于是后面 dispatch emit 的 projection、末尾的 `ResumeInfo` 读到的都是新值。
        let resume_state = self
            .apply_sender_resume_baseline(&session, &mut files, &fetch_plan)
            .await;

        // ▼ B 注册新 epoch actor（commit 前，不含 spawn）。
        self.register_resume_actor(&session, &files, new_epoch, target_peer, &resume_state)
            .await?;

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
        // 发送侧读到的是上面 `apply_sender_resume_baseline` 回填过的值——否则这里会退回
        // DB 那一列的旧值（进程被杀后恒 0），把一次正确的部分续传报成「一点都没传」。
        // 两个真实消费点：MCP 的 `McpResumeResult` 与 IPC 的 `ResumeTransferResult`。
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
        // `mut`：同主动侧，基线要就地回填（dispatch 的 projection 与下面的
        // `build_transfer_resumed_event` 都读这份快照）。
        let mut files = self.store.get_session_files(session_id).await?;
        if let Err(reason) = validate_resume_commit(&session, &files, new_epoch, &fetch_plan) {
            return Ok(TransferResponse::ResumeAck {
                session_id,
                new_epoch,
                accepted: false,
                reason: Some(reason),
            });
        }

        // 基线必须赶在 dispatch 之前——这是本函数与主动侧唯一容易分叉的地方，
        // 而它此前就分叉了：基线埋在 `start_local_resume_actor` 里、落在 dispatch 之后，
        // 于是被动侧 emit 的第一份 projection 仍是旧值（进程被杀的场景恒 0），
        // 用户看到续传从 0% 起步，直到 200ms 节流的第一条 TransferProgress 才纠正。
        // 校验已过（`validate_resume_commit` 在上面），与主动侧「commit 前就写」同纪律。
        let resume_state = self
            .apply_sender_resume_baseline(&session, &mut files, &fetch_plan)
            .await;

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

        self.start_local_resume_actor(
            peer_id,
            &session,
            &files,
            new_epoch,
            fetch_plan,
            &resume_state,
        )
        .await?;
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
                                crate::failure::FailureCode::ResumeRejected { reason },
                            ),
                        },
                    )
                    .await?;
            }
            ResumeRejectReason::PeerUnavailable => {}
        }
        Ok(())
    }

    async fn build_sender_actor_for_resume(
        &self,
        session_id: Uuid,
        peer_id: NodeId,
        files: &[entity::transfer_file::Model],
        resume_state: &HashMap<u32, (u32, u64)>,
    ) -> AppResult<Arc<SenderActor>> {
        let mut prepared_files = build_prepared_files_from_db(files);
        // 回填不可用的 outboard（旧会话无此列 / 空值 / **chunk group 变更后格式作废的存量**）：
        // 按源文件重算并回存，避免逐块重算。
        //
        // 判据是长度而非 `is_empty()`：一份 16KiB 时代写下的 BLOB 非空且看起来合法，
        // 用 `is_empty()` 判会被原样载入、喂进新树后每块 ParentHashMismatch，而回填分支
        // 永不触发——那条会话就**永久**续不上传，且不报错。长度判据同时让 ≤CHUNK_SIZE 的
        // 文件（期望长度恒为 0）不再被误判成缺失而每次 resume 白读一遍整文件。
        for pf in &mut prepared_files {
            if !crate::bao::is_outboard_usable(&pf.outboard, pf.size) {
                let (_, outboard) = crate::bao::build_outboard_from_source(
                    &self.file_access,
                    &pf.source_id,
                    pf.size,
                )
                .await?;
                self.store
                    .save_file_outboard(session_id, pf.file_id as i32, outboard.clone())
                    .await?;
                pf.outboard = outboard;
            }
        }
        Ok(Arc::new(SenderActor::new_with_resume(
            session_id,
            peer_id,
            prepared_files,
            self.file_access.clone(),
            self.events.clone(),
            resume_state,
        )))
    }

    /// 算出发送侧续传基线，**回写 DB 并就地回填内存快照**，返回基线供 actor 复用。
    ///
    /// 一次做三件事是刻意的——三个消费者读的是三个不同的副本，漏掉任何一个都会让
    /// 「续传从 0% 起步」这个症状在那条路径上原样保留：
    ///
    /// | 消费者 | 读哪一份 |
    /// |---|---|
    /// | `dispatch(ResumeCommitted)` emit 的 `TransferProjection` | DB（文件级 SUM） |
    /// | `ResumeInfo.transferred_bytes` / `build_transfer_resumed_event` | 内存 `files` 快照 |
    /// | `ProgressTracker` 的起点 | 返回的基线 map |
    ///
    /// **必须在 `dispatch(ResumeCommitted)` 之前调用。** dispatch 会重读 DB 再 emit 一份
    /// projection；写在它后面的话那一份读到的仍是旧值。移动端 `resume_transfer` 在
    /// `initiate_resume` 返回后还会重读一次，于是「事件里的进度」与「重读到的进度」互相
    /// 打架、看谁后到。此前这段埋在 `build_sender_actor_for_resume` 里，主动侧恰好满足
    /// 时序、被动侧（`handle_resume_commit_impl` 先 dispatch 后建 actor）不满足——
    /// 断言写在文档里而只有一半成立，正是把它提到两个调用点上的原因。
    ///
    /// Receive 方向直接返回空 map：接收侧有自己的事实源（每 10 块落库的 bitmap），
    /// 不需要从对端计划反推，也不该被这里写脏。
    ///
    /// 尽力而为：写失败只影响进度显示，不该把一次本可成功的续传打掉（与
    /// `SenderActor::on_interrupted` 的落进度同纪律）。
    ///
    /// 注意这里会把数字**调小，且可以调到 0**：优雅暂停留下的 `transferred_bytes` 可能比
    /// 对端 checkpoint 高几块（本端已发出、对端尚未落库），基线以对端事实为准。这依赖
    /// `save_sender_file_progress` 的绝对覆盖语义——两处 store 实现此前都过滤零值，
    /// 于是基线为 0 的文件写不下去、projection 先高报再倒退。
    ///
    /// 两侧的失败路径都**不撤销**这次回写——它来自对端 report / fetch_plan，比本地那列的
    /// 旧值更接近事实，留着只会让下次重试的起点更准：
    /// - 主动侧 commit 被拒、actor 被回滚：同上，留着。
    /// - 被动侧 `dispatch` 被 reduce 拒绝（本端仍 Active、未感知中断）：此时本端 tracker
    ///   才是权威，DB 这一列会被后续 `on_completed` / `on_interrupted` 覆盖回去，所以这次
    ///   回写至多让 projection 短暂显示对端视角的数字，不会留下持久的错值。
    ///
    /// ⚠️ 副作用：它改变了本会话 [`build_resume_checkpoint`] 的输出。那条 fallback 在
    /// `completed_ranges` 为空时把 `transferred_bytes` 当作**单个连续 range** 上报，而这里
    /// 写进去的 `bytes_done` 允许中间有洞。今天无人消费（对端只在自己 direction==Send 时
    /// 读对端 checkpoint，而对端此时恒为 Receive），但若将来让接收侧参考发送侧 checkpoint
    /// 做集合减法，会漏掉洞里的块。
    async fn apply_sender_resume_baseline(
        &self,
        session: &entity::transfer_session::Model,
        files: &mut [entity::transfer_file::Model],
        fetch_plan: &[FileRange],
    ) -> HashMap<u32, (u32, u64)> {
        if !matches!(session.direction, TransferDirection::Send) {
            return HashMap::new();
        }
        let resume_state = build_sender_resume_state_from_plan(files, fetch_plan);

        let progress: Vec<(u32, u32, u64)> = resume_state
            .iter()
            .map(|(&file_id, &(chunks_done, transferred))| (file_id, chunks_done, transferred))
            .collect();
        if !progress.is_empty()
            && let Err(error) = self
                .store
                .save_sender_file_progress(session.session_id, &progress)
                .await
        {
            warn!(
                "回写发送方续传基线失败: session={}, error={error}",
                session.session_id
            );
        }

        // 就地回填：DB 写成功与否都要做，两者服务的是不同的读者（见上表），
        // 而内存这份决定了本次调用链后面所有事件里的数字。
        for file in files.iter_mut() {
            if let Some(&(_, transferred)) = resume_state.get(&(file.file_id as u32)) {
                file.transferred_bytes = transferred as i64;
            }
        }

        resume_state
    }

    /// 按方向重建并注册新 epoch actor（**仅构造 + insert，不 spawn**）。
    ///
    /// 主动侧 [`initiate_resume`](Self::initiate_resume)（commit 前）与被动应答侧
    /// [`start_local_resume_actor`](Self::start_local_resume_actor)（transition 后）共用；
    /// `spawn_send_data_channel` 在两侧都作为独立的「激活后」步骤，满足「spawn 在 active
    /// 之后」时序——绝不塞进本 helper，否则主动侧会在 commit/dispatch 前就推送数据面。
    ///
    /// `resume_state` 由 [`apply_sender_resume_baseline`](Self::apply_sender_resume_baseline)
    /// 预先算好（它同时负责落库与回填快照）；Receive 分支忽略它——接收侧有自己的事实源
    /// （每 10 块落库的 bitmap），不需要从对端计划反推。
    async fn register_resume_actor(
        &self,
        session: &entity::transfer_session::Model,
        files: &[entity::transfer_file::Model],
        new_epoch: i64,
        peer_id: NodeId,
        resume_state: &HashMap<u32, (u32, u64)>,
    ) -> AppResult<()> {
        match session.direction {
            TransferDirection::Send => {
                let send_actor = self
                    .build_sender_actor_for_resume(session.session_id, peer_id, files, resume_state)
                    .await?;
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
        Ok(())
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
    async fn start_local_resume_actor(
        &self,
        peer_id: NodeId,
        session: &entity::transfer_session::Model,
        files: &[entity::transfer_file::Model],
        new_epoch: i64,
        fetch_plan: Vec<FileRange>,
        resume_state: &HashMap<u32, (u32, u64)>,
    ) -> AppResult<()> {
        self.register_resume_actor(session, files, new_epoch, peer_id, resume_state)
            .await?;
        if matches!(session.direction, TransferDirection::Send) {
            self.spawn_send_data_channel(session.session_id, new_epoch, fetch_plan);
        }
        Ok(())
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
        .ok_or_else(|| AppError::SessionNotFound("会话不存在".into()))?;

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
            outboard: None,
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
