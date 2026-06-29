//! 传输活动投影 —— 暴露共享 `swarmdrop_core::database::ops::TransferProjection`。
//!
//! 旧的 `MobileSessionStatus`/history item 模型已经不再是移动端状态源。本文件只保留
//! Activity/Recovery 所需的 projection 查询、删除、清空和恢复命令。

use std::sync::Arc;

use sea_orm::EntityTrait;
use uuid::Uuid;

use entity::{SuspendedReason, TerminalReason, TransferDirection, TransferPhase};
use swarmdrop_core::database::ops;
use swarmdrop_core::host::{EventBus, FileAccess};
use swarmdrop_core::transfer::coordinator::TransferCoordinator;

use crate::app::MobileCore;
use crate::error::{FfiError, FfiResult};
use crate::file_access::MobileSaveLocation;

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobileTransferDirection {
    Send,
    Receive,
}

impl From<TransferDirection> for MobileTransferDirection {
    fn from(direction: TransferDirection) -> Self {
        match direction {
            TransferDirection::Send => Self::Send,
            TransferDirection::Receive => Self::Receive,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobileTransferPhase {
    Offered,
    WaitingAccept,
    Active,
    Suspended,
    Terminal,
}

impl From<TransferPhase> for MobileTransferPhase {
    fn from(phase: TransferPhase) -> Self {
        match phase {
            TransferPhase::Offered => Self::Offered,
            TransferPhase::WaitingAccept => Self::WaitingAccept,
            TransferPhase::Active => Self::Active,
            TransferPhase::Suspended => Self::Suspended,
            TransferPhase::Terminal => Self::Terminal,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobileSuspendedReason {
    LocalPaused,
    RemotePaused,
    Interrupted,
    PeerOffline,
    AppRestarted,
}

impl From<SuspendedReason> for MobileSuspendedReason {
    fn from(reason: SuspendedReason) -> Self {
        match reason {
            SuspendedReason::LocalPaused => Self::LocalPaused,
            SuspendedReason::RemotePaused => Self::RemotePaused,
            SuspendedReason::Interrupted => Self::Interrupted,
            SuspendedReason::PeerOffline => Self::PeerOffline,
            SuspendedReason::AppRestarted => Self::AppRestarted,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobileTerminalReason {
    Completed,
    Cancelled,
    Rejected,
    FatalError,
}

impl From<TerminalReason> for MobileTerminalReason {
    fn from(reason: TerminalReason) -> Self {
        match reason {
            TerminalReason::Completed => Self::Completed,
            TerminalReason::Cancelled => Self::Cancelled,
            TerminalReason::Rejected => Self::Rejected,
            TerminalReason::FatalError => Self::FatalError,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileTransferProjectionFile {
    pub file_id: u32,
    pub name: String,
    pub relative_path: String,
    pub size: u64,
    pub transferred_bytes: u64,
}

impl From<ops::TransferProjectionFile> for MobileTransferProjectionFile {
    fn from(file: ops::TransferProjectionFile) -> Self {
        Self {
            file_id: file.file_id.max(0) as u32,
            name: file.name,
            relative_path: file.relative_path,
            size: file.size.max(0) as u64,
            transferred_bytes: file.transferred_bytes.max(0) as u64,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileTransferProjection {
    pub session_id: String,
    pub direction: MobileTransferDirection,
    pub peer_id: String,
    pub peer_name: String,
    pub phase: MobileTransferPhase,
    pub suspended_reason: Option<MobileSuspendedReason>,
    pub terminal_reason: Option<MobileTerminalReason>,
    pub recoverable: bool,
    pub epoch: i64,
    pub total_size: u64,
    pub transferred_bytes: u64,
    pub started_at: i64,
    pub updated_at: i64,
    pub finished_at: Option<i64>,
    pub error_message: Option<String>,
    pub policy_action: Option<String>,
    pub policy_reason: Option<String>,
    pub save_location: Option<MobileSaveLocation>,
    pub files: Vec<MobileTransferProjectionFile>,
}

impl From<ops::TransferProjection> for MobileTransferProjection {
    fn from(projection: ops::TransferProjection) -> Self {
        Self {
            session_id: projection.session_id.to_string(),
            direction: projection.direction.into(),
            peer_id: projection.peer_id,
            peer_name: projection.peer_name,
            phase: projection.phase.into(),
            suspended_reason: projection.suspended_reason.map(Into::into),
            terminal_reason: projection.terminal_reason.map(Into::into),
            recoverable: projection.recoverable,
            epoch: projection.epoch,
            total_size: projection.total_size.max(0) as u64,
            transferred_bytes: projection.transferred_bytes.max(0) as u64,
            started_at: projection.started_at,
            updated_at: projection.updated_at,
            finished_at: projection.finished_at,
            error_message: projection.error_message,
            policy_action: projection.policy_action,
            policy_reason: projection.policy_reason,
            save_location: projection.save_path.map(Into::into),
            files: projection.files.into_iter().map(Into::into).collect(),
        }
    }
}

/// 启动清理（与桌面端 `cleanup_stale_sessions` 对称）：
/// 1. 遗留 active 会话经 core 状态机转 recoverable suspended(AppRestarted)，每次转换都经
///    coordinator dispatch 写 DB + 发 projection（漏发 projection 会让活动列表出现"永远在传"的幽灵条目）；
/// 2. 超过保留期仍未恢复的 recoverable suspended 接收会话经共享 core 原语转 terminal，
///    并用本端 FileAccess 尽力清理遗留 `.part`，防止活动列表与磁盘无限堆积。
pub(crate) async fn reconcile_stale_sessions(
    db: Arc<sea_orm::DatabaseConnection>,
    event_bus: Arc<dyn EventBus>,
    file_access: &Arc<dyn FileAccess>,
) -> FfiResult<usize> {
    let converted = TransferCoordinator::new(db.clone(), event_bus)
        .cleanup_recoverable_sessions()
        .await
        .map_err(FfiError::from)?;

    let reaped = ops::reap_expired_suspended_receives(
        &db,
        swarmdrop_core::transfer::SUSPENDED_RECEIVE_RETENTION_SECS,
    )
    .await
    .map_err(FfiError::from)?;
    swarmdrop_core::transfer::cleanup_expired_part_files(file_access, &reaped).await;

    Ok(converted)
}

fn parse_session_id(s: &str) -> FfiResult<Uuid> {
    Uuid::parse_str(s).map_err(|_| FfiError::Transfer(format!("invalid session_id: {s}")))
}

#[uniffi::export(async_runtime = "tokio")]
impl MobileCore {
    pub async fn get_transfer_projections(&self) -> FfiResult<Vec<MobileTransferProjection>> {
        let db = self.ensure_db().await?;
        let items = ops::get_transfer_projections(&db)
            .await
            .map_err(FfiError::from)?;
        Ok(items.into_iter().map(Into::into).collect())
    }

    pub async fn get_transfer_projection(
        &self,
        session_id: String,
    ) -> FfiResult<Option<MobileTransferProjection>> {
        let session_uuid = parse_session_id(&session_id)?;
        let db = self.ensure_db().await?;
        let item = ops::get_transfer_projection(&db, session_uuid)
            .await
            .map_err(FfiError::from)?;
        Ok(item.map(Into::into))
    }

    pub async fn delete_transfer_record(&self, session_id: String) -> FfiResult<()> {
        let session_uuid = parse_session_id(&session_id)?;
        let db = self.ensure_db().await?;
        ops::delete_session(&db, session_uuid)
            .await
            .map_err(FfiError::from)
    }

    pub async fn clear_transfer_activity(&self) -> FfiResult<()> {
        let db = self.ensure_db().await?;
        ops::clear_all_history(&db).await.map_err(FfiError::from)
    }

    pub async fn resume_transfer(&self, session_id: String) -> FfiResult<MobileTransferProjection> {
        let session_uuid = parse_session_id(&session_id)?;
        let db = self.ensure_db().await?;
        let session = entity::TransferSession::find_by_id(session_uuid)
            .one(&*db)
            .await
            .map_err(|e| FfiError::Database(e.to_string()))?
            .ok_or_else(|| FfiError::Transfer("会话不存在".into()))?;

        let manager = self.transfer_manager_arc().await?;
        match session.direction {
            TransferDirection::Send => {
                manager
                    .initiate_resume_as_sender(session_uuid)
                    .await
                    .map_err(FfiError::from)?;
            }
            TransferDirection::Receive => {
                manager
                    .initiate_resume(session_uuid)
                    .await
                    .map_err(FfiError::from)?;
            }
        }

        let projection = ops::get_transfer_projection(&db, session_uuid)
            .await
            .map_err(FfiError::from)?
            .ok_or_else(|| FfiError::Transfer("会话不存在".into()))?;
        Ok(projection.into())
    }
}
