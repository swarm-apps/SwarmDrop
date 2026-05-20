//! 传输历史 —— 暴露共享 `swarmdrop_core::database::ops` 的查询/清理/恢复能力。
//!
//! 本文件包含：
//! - uniffi Record / Enum 镜像（`MobileSessionStatus` / `MobileTransferHistoryItem` / ...）
//! - `entity::transfer_session::ModelEx` → mobile 类型的 `From` 转换
//! - `MobileCore` 上的 5 个查询/清理/恢复方法
//! - `reconcile_stale_sessions` —— 启动时把残留 `Transferring` 状态标记为 failed
//!
//! 业务逻辑全部在共享 crate，本文件只做 ABI 桥接。

use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use uuid::Uuid;

use entity::SessionStatus;
use swarmdrop_core::database::ops;

use crate::app::MobileCore;
use crate::error::{ERROR_APP_INTERRUPTED, FfiError, FfiResult};
use crate::events::MobileTransferResumedFile;

// ─────────────── uniffi 类型 ───────────────

/// DB 层会话状态镜像（5 个变种，对齐 `entity::SessionStatus`）。
///
/// `pending` / `waiting_accept` 是 RN 活跃 session 的内存 UI 中间态，
/// 不进 DB，也不在这层暴露。
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobileSessionStatus {
    Transferring,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

impl From<SessionStatus> for MobileSessionStatus {
    fn from(s: SessionStatus) -> Self {
        match s {
            SessionStatus::Transferring => Self::Transferring,
            SessionStatus::Paused => Self::Paused,
            SessionStatus::Completed => Self::Completed,
            SessionStatus::Failed => Self::Failed,
            SessionStatus::Cancelled => Self::Cancelled,
        }
    }
}

impl From<MobileSessionStatus> for SessionStatus {
    fn from(s: MobileSessionStatus) -> Self {
        match s {
            MobileSessionStatus::Transferring => Self::Transferring,
            MobileSessionStatus::Paused => Self::Paused,
            MobileSessionStatus::Completed => Self::Completed,
            MobileSessionStatus::Failed => Self::Failed,
            MobileSessionStatus::Cancelled => Self::Cancelled,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileTransferHistoryFile {
    pub file_id: u32,
    pub name: String,
    pub relative_path: String,
    pub size: u64,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileTransferHistoryItem {
    pub session_id: String,
    /// "send" | "receive"
    pub direction: String,
    pub peer_id: String,
    pub peer_name: String,
    pub status: MobileSessionStatus,
    pub files: Vec<MobileTransferHistoryFile>,
    pub total_size: u64,
    pub transferred_bytes: u64,
    pub error_message: Option<String>,
    /// 接收方的保存路径（CoreSaveLocation::Path 透传）；发送方为 None
    pub save_path: Option<String>,
    /// Unix ms
    pub started_at: i64,
    /// Unix ms
    pub updated_at: i64,
    /// Unix ms；进行中为 None
    pub finished_at: Option<i64>,
}

impl From<ops::TransferHistoryFile> for MobileTransferHistoryFile {
    fn from(f: ops::TransferHistoryFile) -> Self {
        Self {
            file_id: f.file_id.max(0) as u32,
            name: f.name,
            relative_path: f.relative_path,
            size: f.size.max(0) as u64,
        }
    }
}

impl From<ops::TransferHistoryItem> for MobileTransferHistoryItem {
    fn from(item: ops::TransferHistoryItem) -> Self {
        use swarmdrop_core::host::CoreSaveLocation;
        Self {
            session_id: item.session_id.to_string(),
            direction: format!("{:?}", item.direction).to_lowercase(),
            peer_id: item.peer_id,
            peer_name: item.peer_name,
            status: item.status.into(),
            files: item.files.into_iter().map(Into::into).collect(),
            total_size: item.total_size.max(0) as u64,
            transferred_bytes: item.transferred_bytes.max(0) as u64,
            error_message: item.error_message,
            save_path: item.save_path.map(|loc| match loc {
                CoreSaveLocation::Path { path } => path,
            }),
            started_at: item.started_at,
            updated_at: item.updated_at,
            finished_at: item.finished_at,
        }
    }
}

/// `resume_transfer` 的返回值
#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileResumeTransferResult {
    pub session_id: String,
    /// "send" | "receive"
    pub direction: String,
    pub peer_id: String,
    pub peer_name: String,
    pub files: Vec<MobileTransferResumedFile>,
    pub total_size: u64,
    pub transferred_bytes: u64,
}

// ─────────────── reconcile（启动时清理脏状态） ───────────────

/// 启动节点时调用：把 DB 中残留为 `Transferring` 状态的会话（进程死亡留下的）
/// 标记为 failed，错误消息为 `app_interrupted`。
///
/// `Paused` 是用户主动暂停的合法状态，**不**参与 reconcile。
/// 终态 Completed/Failed/Cancelled 自然也不动。
pub(crate) async fn reconcile_stale_sessions(db: &DatabaseConnection) -> FfiResult<()> {
    let stale = entity::TransferSession::find()
        .filter(entity::transfer_session::Column::Status.eq(SessionStatus::Transferring))
        .all(db)
        .await
        .map_err(|e| FfiError::Database(e.to_string()))?;

    let count = stale.len();
    for s in stale {
        let session_id = s.session_id;
        if let Err(err) = ops::mark_session_failed(db, session_id, ERROR_APP_INTERRUPTED).await {
            tracing::warn!(
                "reconcile: 标记 {} 为 failed 时出错: {}",
                session_id,
                err
            );
            continue;
        }
        tracing::warn!("reconciled stale session {} (was Transferring)", session_id);
    }
    if count > 0 {
        tracing::info!("启动 reconcile 完成，清理 {} 条脏 session", count);
    }
    Ok(())
}

// ─────────────── MobileCore 方法 ───────────────

fn parse_session_id(s: &str) -> FfiResult<Uuid> {
    Uuid::parse_str(s).map_err(|_| FfiError::Transfer(format!("invalid session_id: {s}")))
}

#[uniffi::export(async_runtime = "tokio")]
impl MobileCore {
    /// 查询传输历史列表（可选按状态过滤），按 started_at 降序。
    pub async fn list_transfer_history(
        &self,
        status_filter: Option<MobileSessionStatus>,
    ) -> FfiResult<Vec<MobileTransferHistoryItem>> {
        let db = self.ensure_db().await?;
        let filter: Option<SessionStatus> = status_filter.map(Into::into);
        let items = ops::get_transfer_history(&db, filter)
            .await
            .map_err(FfiError::from)?;
        Ok(items.into_iter().map(Into::into).collect())
    }

    /// 查询单个会话详情；找不到时抛 `FfiError::Transfer("会话不存在")`。
    pub async fn get_transfer_session_detail(
        &self,
        session_id: String,
    ) -> FfiResult<MobileTransferHistoryItem> {
        let session_uuid = parse_session_id(&session_id)?;
        let db = self.ensure_db().await?;
        let item = ops::get_session_detail(&db, session_uuid)
            .await
            .map_err(FfiError::from)?;
        Ok(item.into())
    }

    /// 删除单个会话（级联删除文件）；不存在时静默跳过。
    pub async fn delete_transfer_session(&self, session_id: String) -> FfiResult<()> {
        let session_uuid = parse_session_id(&session_id)?;
        let db = self.ensure_db().await?;
        ops::delete_session(&db, session_uuid)
            .await
            .map_err(FfiError::from)
    }

    /// 清空全部历史（含文件子记录）。
    pub async fn clear_transfer_history(&self) -> FfiResult<()> {
        let db = self.ensure_db().await?;
        ops::clear_all_history(&db).await.map_err(FfiError::from)
    }

    /// 恢复传输：根据 DB 中记录的方向分发到 sender/receiver resume 流程。
    ///
    /// 返回值包含新协商出的会话元信息；对端离线 / 文件被改动等失败由
    /// 共享 crate 的 `initiate_resume*` 自己负责，RN 端 catch FfiError toast。
    pub async fn resume_transfer(
        &self,
        session_id: String,
    ) -> FfiResult<MobileResumeTransferResult> {
        let session_uuid = parse_session_id(&session_id)?;
        let db = self.ensure_db().await?;

        let session = entity::TransferSession::find_by_id(session_uuid)
            .one(&*db)
            .await
            .map_err(|e| FfiError::Database(e.to_string()))?
            .ok_or_else(|| FfiError::Transfer("会话不存在".into()))?;

        let manager = self.transfer_manager_arc().await?;

        let (resume_info, direction_str) = match session.direction {
            entity::TransferDirection::Send => (
                manager
                    .initiate_resume_as_sender(session_uuid)
                    .await
                    .map_err(FfiError::from)?,
                "send",
            ),
            entity::TransferDirection::Receive => (
                manager
                    .initiate_resume(session_uuid)
                    .await
                    .map_err(FfiError::from)?,
                "receive",
            ),
        };

        Ok(MobileResumeTransferResult {
            session_id: session_uuid.to_string(),
            direction: direction_str.to_string(),
            peer_id: resume_info.peer_id,
            peer_name: resume_info.peer_name,
            files: resume_info
                .files
                .into_iter()
                .map(|f| MobileTransferResumedFile {
                    file_id: f.file_id.max(0) as u32,
                    name: f.name,
                    relative_path: f.relative_path,
                    size: f.size.max(0) as u64,
                    is_directory: false,
                })
                .collect(),
            total_size: resume_info.total_size.max(0) as u64,
            transferred_bytes: resume_info.transferred_bytes.max(0) as u64,
        })
    }
}
