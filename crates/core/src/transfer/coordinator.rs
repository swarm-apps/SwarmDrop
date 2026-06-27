//! 传输生命周期协调器（状态机核心）。
//!
//! [`reduce`] 是纯函数：`(当前状态, 输入) → Some(新状态)` 表示状态转换，
//! `None` 表示忽略该输入（旧 epoch 的迟到消息、terminal 后的事件、非法转换）。
//! 副作用（写 DB、发前端 projection）由 Coordinator 在 reduce 之外执行——
//! 这样状态转换逻辑可独立单元测试，不依赖 DB / 网络。
//!
//! 设计见 `openspec/changes/redesign-transfer-lifecycle/design.md` D1-D4。

use std::sync::Arc;

use entity::{SuspendedReason, TerminalReason, TransferPhase};
use sea_orm::DatabaseConnection;
use uuid::Uuid;

use crate::AppResult;
use crate::host::{CoreEvent, EventBus};

/// 传输生命周期状态（镜像 entity 持久化字段）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferState {
    pub phase: TransferPhase,
    pub suspended_reason: Option<SuspendedReason>,
    pub terminal_reason: Option<TerminalReason>,
    pub epoch: i64,
    pub recoverable: bool,
}

impl TransferState {
    /// 收到对端 Offer、等待本地用户接受或拒绝。
    pub fn offered(epoch: i64) -> Self {
        Self {
            phase: TransferPhase::Offered,
            suspended_reason: None,
            terminal_reason: None,
            epoch,
            recoverable: true,
        }
    }

    /// 发送方 Offer 已发出、等待对端接受。
    pub fn waiting_accept(epoch: i64) -> Self {
        Self {
            phase: TransferPhase::WaitingAccept,
            suspended_reason: None,
            terminal_reason: None,
            epoch,
            recoverable: true,
        }
    }

    /// active 传输中。
    pub fn active(epoch: i64) -> Self {
        Self {
            phase: TransferPhase::Active,
            suspended_reason: None,
            terminal_reason: None,
            epoch,
            recoverable: true,
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self.phase, TransferPhase::Terminal)
    }

    pub fn is_suspended(&self) -> bool {
        matches!(self.phase, TransferPhase::Suspended)
    }

    pub fn is_active(&self) -> bool {
        matches!(self.phase, TransferPhase::Active)
    }

    fn suspended(epoch: i64, reason: SuspendedReason) -> Self {
        Self {
            phase: TransferPhase::Suspended,
            suspended_reason: Some(reason),
            terminal_reason: None,
            epoch,
            recoverable: true,
        }
    }

    fn terminal(epoch: i64, reason: TerminalReason) -> Self {
        Self {
            phase: TransferPhase::Terminal,
            suspended_reason: None,
            terminal_reason: Some(reason),
            epoch,
            recoverable: false,
        }
    }
}

impl From<&entity::transfer_session::Model> for TransferState {
    fn from(m: &entity::transfer_session::Model) -> Self {
        Self {
            phase: m.phase.clone(),
            suspended_reason: m.suspended_reason.clone(),
            terminal_reason: m.terminal_reason.clone(),
            epoch: m.epoch,
            recoverable: m.recoverable,
        }
    }
}

/// 用户发起的命令。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserCommand {
    Pause,
    Cancel,
    /// 触发恢复探测（不直接转状态，由后续 [`NetworkSignal::ResumeCommitted`] 转 active）。
    Resume,
    Accept,
    Reject,
}

/// actor（sender/receiver）报告的事件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActorReport {
    /// 进度更新（不改 phase，进度字段单独处理）。
    Progress,
    /// checkpoint 已 flush（不改 phase）。
    CheckpointFlushed,
    /// 所有文件传输完成。
    Completed,
    /// 不可恢复错误（源文件变更、校验失败、协议不兼容）。
    FatalError(String),
}

/// 网络 / 对端信号。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkSignal {
    /// 对端暂停。
    RemotePaused,
    /// 数据请求因连接丢失失败 / 底层连接断开。
    Interrupted,
    /// 对端离线。
    PeerOffline,
    /// 对端取消。
    RemoteCancelled,
    /// 对端接受 Offer。
    OfferAccepted,
    /// 对端拒绝 Offer。
    OfferRejected,
    /// 恢复提交成功，生成新 epoch。
    ResumeCommitted { new_epoch: i64 },
}

/// 启动清理信号。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartupSignal {
    /// 启动时发现遗留 active session（应转为 recoverable suspended）。
    FoundActiveSession,
}

/// Coordinator 统一输入。actor / network 事件携带 epoch，用于防旧消息污染。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoordinatorInput {
    User(UserCommand),
    Actor { epoch: i64, report: ActorReport },
    Network { epoch: i64, signal: NetworkSignal },
    Startup(StartupSignal),
}

/// 状态机 reducer（纯函数）。
///
/// 返回 `Some(新状态)` 表示发生转换（调用方据此写 DB + 发 projection），
/// `None` 表示忽略输入：旧 epoch 迟到消息、terminal 后的事件、当前 phase 下的非法转换。
pub fn reduce(state: &TransferState, input: &CoordinatorInput) -> Option<TransferState> {
    // epoch 校验：actor / network 事件 epoch < current 一律忽略（D3 防旧消息）。
    match input {
        CoordinatorInput::Actor { epoch, .. } | CoordinatorInput::Network { epoch, .. }
            if *epoch < state.epoch =>
        {
            return None;
        }
        _ => {}
    }

    // terminal 不可逆：completed / cancelled / rejected / fatal_error 之后不再转换。
    if state.is_terminal() {
        return None;
    }

    match input {
        CoordinatorInput::User(cmd) => reduce_user(state, cmd),
        CoordinatorInput::Actor { report, .. } => reduce_actor(state, report),
        CoordinatorInput::Network { signal, .. } => reduce_network(state, signal),
        CoordinatorInput::Startup(sig) => reduce_startup(state, sig),
    }
}

fn reduce_user(state: &TransferState, cmd: &UserCommand) -> Option<TransferState> {
    match cmd {
        // 用户暂停：仅 active 可暂停。
        UserCommand::Pause if state.is_active() => Some(TransferState::suspended(
            state.epoch,
            SuspendedReason::LocalPaused,
        )),
        // 取消是强制终止：任何非 terminal 状态 → cancelled。
        UserCommand::Cancel => Some(TransferState::terminal(
            state.epoch,
            TerminalReason::Cancelled,
        )),
        // 接受 Offer：offered / waiting_accept → active。
        UserCommand::Accept
            if matches!(
                state.phase,
                TransferPhase::Offered | TransferPhase::WaitingAccept
            ) =>
        {
            Some(TransferState::active(state.epoch))
        }
        // 拒绝 Offer：offered / waiting_accept → rejected。
        UserCommand::Reject
            if matches!(
                state.phase,
                TransferPhase::Offered | TransferPhase::WaitingAccept
            ) =>
        {
            Some(TransferState::terminal(
                state.epoch,
                TerminalReason::Rejected,
            ))
        }
        // 恢复走探测协议，不在 reducer 直接转状态（由 ResumeCommitted 转 active）。
        _ => None,
    }
}

fn reduce_actor(state: &TransferState, report: &ActorReport) -> Option<TransferState> {
    match report {
        // 进度 / checkpoint 不改 phase。
        ActorReport::Progress | ActorReport::CheckpointFlushed => None,
        // 完成：active → completed。
        ActorReport::Completed if state.is_active() => Some(TransferState::terminal(
            state.epoch,
            TerminalReason::Completed,
        )),
        ActorReport::Completed => None,
        // 不可恢复错误：任何非 terminal → fatal_error。
        ActorReport::FatalError(_) => Some(TransferState::terminal(
            state.epoch,
            TerminalReason::FatalError,
        )),
    }
}

fn reduce_network(state: &TransferState, signal: &NetworkSignal) -> Option<TransferState> {
    match signal {
        NetworkSignal::RemotePaused if state.is_active() => Some(TransferState::suspended(
            state.epoch,
            SuspendedReason::RemotePaused,
        )),
        NetworkSignal::Interrupted if state.is_active() => Some(TransferState::suspended(
            state.epoch,
            SuspendedReason::Interrupted,
        )),
        NetworkSignal::PeerOffline if state.is_active() => Some(TransferState::suspended(
            state.epoch,
            SuspendedReason::PeerOffline,
        )),
        NetworkSignal::RemoteCancelled => Some(TransferState::terminal(
            state.epoch,
            TerminalReason::Cancelled,
        )),
        NetworkSignal::OfferAccepted if state.phase == TransferPhase::WaitingAccept => {
            Some(TransferState::active(state.epoch))
        }
        NetworkSignal::OfferRejected
            if matches!(
                state.phase,
                TransferPhase::Offered | TransferPhase::WaitingAccept
            ) =>
        {
            Some(TransferState::terminal(
                state.epoch,
                TerminalReason::Rejected,
            ))
        }
        // 恢复提交：suspended + recoverable → active with new epoch。
        NetworkSignal::ResumeCommitted { new_epoch }
            if state.is_suspended() && state.recoverable && *new_epoch > state.epoch =>
        {
            Some(TransferState::active(*new_epoch))
        }
        _ => None,
    }
}

fn reduce_startup(state: &TransferState, sig: &StartupSignal) -> Option<TransferState> {
    match sig {
        // 启动清理：遗留 active → recoverable suspended(app_restarted)，
        // 不伪装成 paused / failed。
        StartupSignal::FoundActiveSession if state.is_active() => Some(TransferState::suspended(
            state.epoch,
            SuspendedReason::AppRestarted,
        )),
        _ => None,
    }
}

/// 传输协调器运行时入口。
///
/// [`TransferCoordinator::dispatch`] 是状态机的唯一持久化入口：
/// load 当前状态 → [`reduce`]（纯函数）→ 写 DB。reducer 保持纯、可独立测试，
/// dispatch 负责 I/O 副作用（DB 读写）。
pub struct TransferCoordinator {
    db: Arc<DatabaseConnection>,
    event_bus: Arc<dyn EventBus>,
}

impl TransferCoordinator {
    pub fn new(db: Arc<DatabaseConnection>, event_bus: Arc<dyn EventBus>) -> Self {
        Self { db, event_bus }
    }

    /// 处理一个输入：load 当前状态 → reduce → 持久化 → 发 projection 事件。
    ///
    /// 返回 `Some(新状态)` 表示发生转换（调用方据此驱动 actor），
    /// `None` 表示输入被忽略（旧 epoch、terminal 后、非法转换）或 session 不存在。
    /// 这是状态变化的唯一持久化 + 前端投影入口。
    pub async fn dispatch(
        &self,
        session_id: Uuid,
        input: CoordinatorInput,
    ) -> AppResult<Option<TransferState>> {
        let Some(session) = crate::database::ops::find_session(&self.db, session_id).await? else {
            return Ok(None);
        };
        self.apply_input(&session, session_id, &input).await
    }

    /// 入站网络信号（对端 Cancel/Pause）的便捷入口：当前 req_resp 控制消息不携带
    /// epoch，用 session 当前 epoch dispatch（等价无 stale 保护——待数据面协议
    /// 在帧里带 epoch 后收紧）。单次 load 后复用 [`apply_input`](Self::apply_input)。
    pub async fn dispatch_network_current(
        &self,
        session_id: Uuid,
        signal: NetworkSignal,
    ) -> AppResult<Option<TransferState>> {
        let Some(session) = crate::database::ops::find_session(&self.db, session_id).await? else {
            return Ok(None);
        };
        let input = CoordinatorInput::Network {
            epoch: session.epoch,
            signal,
        };
        self.apply_input(&session, session_id, &input).await
    }

    /// reduce + 持久化 + 发 projection（dispatch 系列共用，session 已加载、不二次 SELECT）。
    async fn apply_input(
        &self,
        session: &entity::transfer_session::Model,
        session_id: Uuid,
        input: &CoordinatorInput,
    ) -> AppResult<Option<TransferState>> {
        let current = TransferState::from(session);
        match reduce(&current, input) {
            Some(new_state) => {
                crate::database::ops::apply_transition(&self.db, session, &new_state).await?;
                if let Some(projection) =
                    crate::database::ops::get_transfer_projection(&self.db, session_id).await?
                {
                    self.event_bus
                        .publish(CoreEvent::TransferProjection { projection })
                        .await?;
                }
                Ok(Some(new_state))
            }
            None => Ok(None),
        }
    }

    /// 启动清理：把上次运行遗留的 active session 统一转为
    /// recoverable suspended(AppRestarted)，而不是 paused / failed 混用（task 2.5）。
    ///
    /// 复用状态机：对每个遗留 active session dispatch `Startup(FoundActiveSession)`，
    /// reducer 只把 active 转 suspended、其余 phase 自然 no-op，每次转换都经
    /// [`dispatch`](Self::dispatch) 写 DB + 发 projection。返回被转换的会话数。
    pub async fn cleanup_recoverable_sessions(&self) -> AppResult<usize> {
        let ids = crate::database::ops::find_active_session_ids(&self.db).await?;
        let mut converted = 0;
        for id in ids {
            let transitioned = self
                .dispatch(
                    id,
                    CoordinatorInput::Startup(StartupSignal::FoundActiveSession),
                )
                .await?
                .is_some();
            if transitioned {
                converted += 1;
            }
        }
        Ok(converted)
    }

    /// 直接发当前 session 的 projection（用于已由 `mark_*` 写好 phase、不经 reduce
    /// 的过渡路径，如带 file 副作用的 complete/failed）。
    pub async fn publish_projection(&self, session_id: Uuid) -> AppResult<()> {
        if let Some(projection) =
            crate::database::ops::get_transfer_projection(&self.db, session_id).await?
        {
            self.event_bus
                .publish(CoreEvent::TransferProjection { projection })
                .await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn active(epoch: i64) -> TransferState {
        TransferState::active(epoch)
    }

    #[test]
    fn user_pause_active_to_suspended_local() {
        let s = reduce(&active(1), &CoordinatorInput::User(UserCommand::Pause)).unwrap();
        assert_eq!(s.phase, TransferPhase::Suspended);
        assert_eq!(s.suspended_reason, Some(SuspendedReason::LocalPaused));
        assert!(s.recoverable);
    }

    #[test]
    fn network_interrupt_active_to_suspended_recoverable() {
        let s = reduce(
            &active(1),
            &CoordinatorInput::Network {
                epoch: 1,
                signal: NetworkSignal::Interrupted,
            },
        )
        .unwrap();
        assert_eq!(s.suspended_reason, Some(SuspendedReason::Interrupted));
        assert!(s.recoverable);
    }

    #[test]
    fn user_cancel_to_terminal_not_recoverable() {
        let s = reduce(&active(1), &CoordinatorInput::User(UserCommand::Cancel)).unwrap();
        assert_eq!(s.phase, TransferPhase::Terminal);
        assert_eq!(s.terminal_reason, Some(TerminalReason::Cancelled));
        assert!(!s.recoverable);
    }

    #[test]
    fn fatal_error_to_terminal() {
        let s = reduce(
            &active(1),
            &CoordinatorInput::Actor {
                epoch: 1,
                report: ActorReport::FatalError("source changed".into()),
            },
        )
        .unwrap();
        assert_eq!(s.terminal_reason, Some(TerminalReason::FatalError));
        assert!(!s.recoverable);
    }

    #[test]
    fn startup_active_to_suspended_app_restarted() {
        let s = reduce(
            &active(1),
            &CoordinatorInput::Startup(StartupSignal::FoundActiveSession),
        )
        .unwrap();
        assert_eq!(s.suspended_reason, Some(SuspendedReason::AppRestarted));
    }

    #[test]
    fn old_epoch_actor_event_is_ignored() {
        // current epoch=5，迟到的旧 epoch=3 actor 事件必须忽略
        let s = active(5);
        let out = reduce(
            &s,
            &CoordinatorInput::Actor {
                epoch: 3,
                report: ActorReport::Completed,
            },
        );
        assert!(out.is_none(), "旧 epoch actor 事件必须被忽略");
    }

    #[test]
    fn terminal_is_irreversible() {
        let terminal = TransferState::terminal(1, TerminalReason::Cancelled);
        // terminal 后任何输入都不转换
        assert!(reduce(&terminal, &CoordinatorInput::User(UserCommand::Resume)).is_none());
        assert!(
            reduce(
                &terminal,
                &CoordinatorInput::Network {
                    epoch: 1,
                    signal: NetworkSignal::ResumeCommitted { new_epoch: 2 }
                }
            )
            .is_none()
        );
    }

    #[test]
    fn resume_commit_suspended_to_active_new_epoch() {
        let suspended = TransferState::suspended(2, SuspendedReason::Interrupted);
        let s = reduce(
            &suspended,
            &CoordinatorInput::Network {
                epoch: 2,
                signal: NetworkSignal::ResumeCommitted { new_epoch: 3 },
            },
        )
        .unwrap();
        assert_eq!(s.phase, TransferPhase::Active);
        assert_eq!(s.epoch, 3);
        assert!(s.recoverable);
    }

    #[test]
    fn resume_commit_must_advance_epoch() {
        let suspended = TransferState::suspended(2, SuspendedReason::Interrupted);
        let out = reduce(
            &suspended,
            &CoordinatorInput::Network {
                epoch: 2,
                signal: NetworkSignal::ResumeCommitted { new_epoch: 2 },
            },
        );
        assert!(out.is_none(), "恢复提交必须推进到更大的 epoch");
    }

    #[test]
    fn pause_only_affects_active() {
        // suspended 状态再 pause 应忽略
        let suspended = TransferState::suspended(1, SuspendedReason::Interrupted);
        assert!(reduce(&suspended, &CoordinatorInput::User(UserCommand::Pause)).is_none());
    }

    #[test]
    fn accept_waiting_to_active() {
        let s = reduce(
            &TransferState::waiting_accept(1),
            &CoordinatorInput::User(UserCommand::Accept),
        )
        .unwrap();
        assert_eq!(s.phase, TransferPhase::Active);
    }

    #[test]
    fn accept_offered_to_active() {
        let s = reduce(
            &TransferState::offered(1),
            &CoordinatorInput::User(UserCommand::Accept),
        )
        .unwrap();
        assert_eq!(s.phase, TransferPhase::Active);
    }

    #[test]
    fn reject_offer_states_to_terminal_rejected() {
        for state in [TransferState::offered(1), TransferState::waiting_accept(1)] {
            let s = reduce(&state, &CoordinatorInput::User(UserCommand::Reject)).unwrap();
            assert_eq!(s.phase, TransferPhase::Terminal);
            assert_eq!(s.terminal_reason, Some(TerminalReason::Rejected));
            assert!(!s.recoverable);
        }
    }

    #[test]
    fn network_offer_result_updates_waiting_accept() {
        let accepted = reduce(
            &TransferState::waiting_accept(1),
            &CoordinatorInput::Network {
                epoch: 1,
                signal: NetworkSignal::OfferAccepted,
            },
        )
        .unwrap();
        assert_eq!(accepted.phase, TransferPhase::Active);

        let rejected = reduce(
            &TransferState::waiting_accept(1),
            &CoordinatorInput::Network {
                epoch: 1,
                signal: NetworkSignal::OfferRejected,
            },
        )
        .unwrap();
        assert_eq!(rejected.phase, TransferPhase::Terminal);
        assert_eq!(rejected.terminal_reason, Some(TerminalReason::Rejected));
    }

    #[test]
    fn remote_pause_and_peer_offline_are_recoverable_suspended() {
        let remote_paused = reduce(
            &active(1),
            &CoordinatorInput::Network {
                epoch: 1,
                signal: NetworkSignal::RemotePaused,
            },
        )
        .unwrap();
        assert_eq!(
            remote_paused.suspended_reason,
            Some(SuspendedReason::RemotePaused)
        );
        assert!(remote_paused.recoverable);

        let peer_offline = reduce(
            &active(1),
            &CoordinatorInput::Network {
                epoch: 1,
                signal: NetworkSignal::PeerOffline,
            },
        )
        .unwrap();
        assert_eq!(
            peer_offline.suspended_reason,
            Some(SuspendedReason::PeerOffline)
        );
        assert!(peer_offline.recoverable);
    }

    #[test]
    fn remote_cancel_is_terminal_cancelled_from_non_terminal_states() {
        for state in [
            TransferState::offered(1),
            TransferState::waiting_accept(1),
            TransferState::active(1),
            TransferState::suspended(1, SuspendedReason::Interrupted),
        ] {
            let s = reduce(
                &state,
                &CoordinatorInput::Network {
                    epoch: 1,
                    signal: NetworkSignal::RemoteCancelled,
                },
            )
            .unwrap();
            assert_eq!(s.phase, TransferPhase::Terminal);
            assert_eq!(s.terminal_reason, Some(TerminalReason::Cancelled));
            assert!(!s.recoverable);
        }
    }
}
