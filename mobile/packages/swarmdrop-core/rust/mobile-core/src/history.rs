//! 传输活动投影 —— 暴露共享 `swarmdrop_core::transfer::store::TransferProjection`。
//!
//! 旧的 `MobileSessionStatus`/history item 模型已经不再是移动端状态源。本文件只保留
//! Activity/Recovery 所需的 projection 查询、删除、清空、源路径和恢复命令。
//!
//! 历史管理一律经 `TransferManager::store()` 取回持久化端口，不再直连 SeaORM 自由函数
//! —— 端口有出口后，宿主侧就不该再存第二份数据库句柄做传输查询。

use std::sync::Arc;

use uuid::Uuid;

use entity::{SuspendedReason, TerminalReason, TransferDirection, TransferPhase};
use swarmdrop_core::host::{EventBus, FileAccess};
use swarmdrop_core::transfer::coordinator::TransferCoordinator;
use swarmdrop_core::transfer::failure::FailureCode;
use swarmdrop_core::transfer::protocol::ResumeRejectReason;
use swarmdrop_core::transfer::store::{TransferProjection, TransferProjectionFile, TransferStore};

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
    /// 入站 offer 的决策窗口耗尽，本端从未作答（≠ 用户拒绝，见 `entity::TerminalReason`）。
    Expired,
}

impl From<TerminalReason> for MobileTerminalReason {
    fn from(reason: TerminalReason) -> Self {
        match reason {
            TerminalReason::Completed => Self::Completed,
            TerminalReason::Cancelled => Self::Cancelled,
            TerminalReason::Rejected => Self::Rejected,
            TerminalReason::FatalError => Self::FatalError,
            TerminalReason::Expired => Self::Expired,
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

impl From<TransferProjectionFile> for MobileTransferProjectionFile {
    fn from(file: TransferProjectionFile) -> Self {
        // 穷尽解构 drift guard：上游给 `TransferProjectionFile` 加字段时这里会编译失败。
        let TransferProjectionFile {
            file_id,
            name,
            relative_path,
            size,
            transferred_bytes,
        } = file;
        Self {
            file_id: file_id.max(0) as u32,
            name,
            relative_path,
            size: size.max(0) as u64,
            transferred_bytes: transferred_bytes.max(0) as u64,
        }
    }
}

/// 失败判别码的 uniffi 镜像（见 `swarmdrop_transfer::failure::FailureCode`）。
///
/// 它取代的是一个直达 UI 的自由中文串。TS 侧过去用 9 条**英文**关键词正则去猜它的语义，
/// 而消息里拼着文件名——一个叫 `Q3-cancel.xlsx` 的文件校验失败会被显示成「传输已取消」。
/// 判别码把「是什么失败」和「怎么措辞」分开之后，那种猜测彻底没有存在的余地。
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum MobileFailureCode {
    FileFinalizeFailed {
        file_name: String,
    },
    SessionExpired {
        retention_days: u32,
    },
    ResumeRejected {
        reason: MobileResumeRejectReason,
    },
    OfferFailed,
    /// 判别码引入之前落库的自由文本，原样透传给 UI。
    Legacy {
        message: String,
    },
}

/// 续传被对端拒绝的原因（`swarmdrop_transfer::protocol::ResumeRejectReason` 的镜像）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobileResumeRejectReason {
    Cancelled,
    FatalError,
    SourceModified,
    CheckpointInvalid,
    PeerUnavailable,
    SessionNotFound,
}

impl From<ResumeRejectReason> for MobileResumeRejectReason {
    fn from(reason: ResumeRejectReason) -> Self {
        match reason {
            ResumeRejectReason::Cancelled => Self::Cancelled,
            ResumeRejectReason::FatalError => Self::FatalError,
            ResumeRejectReason::SourceModified => Self::SourceModified,
            ResumeRejectReason::CheckpointInvalid => Self::CheckpointInvalid,
            ResumeRejectReason::PeerUnavailable => Self::PeerUnavailable,
            ResumeRejectReason::SessionNotFound => Self::SessionNotFound,
        }
    }
}

impl From<FailureCode> for MobileFailureCode {
    fn from(code: FailureCode) -> Self {
        match code {
            FailureCode::FileFinalizeFailed { file_name } => Self::FileFinalizeFailed { file_name },
            FailureCode::SessionExpired { retention_days } => {
                Self::SessionExpired { retention_days }
            }
            FailureCode::ResumeRejected { reason } => Self::ResumeRejected {
                reason: reason.into(),
            },
            FailureCode::OfferFailed => Self::OfferFailed,
            FailureCode::Legacy { message } => Self::Legacy { message },
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
    pub failure: Option<MobileFailureCode>,
    pub policy_action: Option<String>,
    pub policy_reason: Option<String>,
    pub save_location: Option<MobileSaveLocation>,
    /// 「打开文件夹」应定位的真实容器目录 URI(收到内容实际所在文件夹);缺失时前端回退
    /// `save_location`。由 core 从各文件 local_dir 事实源计算,不做相对路径拼接推导。
    pub content_root: Option<String>,
    pub files: Vec<MobileTransferProjectionFile>,
}

impl From<TransferProjection> for MobileTransferProjection {
    fn from(projection: TransferProjection) -> Self {
        // 穷尽解构 drift guard：上游给 `TransferProjection` 加字段时这里会编译失败。
        let TransferProjection {
            session_id,
            direction,
            peer_id,
            peer_name,
            phase,
            suspended_reason,
            terminal_reason,
            recoverable,
            epoch,
            total_size,
            transferred_bytes,
            started_at,
            updated_at,
            finished_at,
            failure,
            policy_action,
            policy_reason,
            save_path,
            content_root,
            files,
        } = projection;
        Self {
            session_id: session_id.to_string(),
            direction: direction.into(),
            peer_id,
            peer_name,
            phase: phase.into(),
            suspended_reason: suspended_reason.map(Into::into),
            terminal_reason: terminal_reason.map(Into::into),
            recoverable,
            epoch,
            total_size: total_size.max(0) as u64,
            transferred_bytes: transferred_bytes.max(0) as u64,
            started_at,
            updated_at,
            finished_at,
            failure: failure.map(Into::into),
            policy_action,
            policy_reason,
            save_location: save_path.map(Into::into),
            content_root,
            files: files.into_iter().map(Into::into).collect(),
        }
    }
}

/// 启动清理（与桌面端 `cleanup_stale_sessions` 对称）：
/// 1. 遗留 active 会话经 core 状态机转 recoverable suspended(AppRestarted)，每次转换都经
///    coordinator dispatch 写 DB + 发 projection（漏发 projection 会让活动列表出现"永远在传"的幽灵条目）；
/// 2. 超过保留期仍未恢复的 recoverable suspended 接收会话经共享 core 原语转 terminal，
///    并用本端 FileAccess 尽力清理遗留 `.part`，防止活动列表与磁盘无限堆积。
///
/// 本函数跑在 `start_node` 里、节点尚未起来，拿不到 `TransferManager::store()`，
/// 故由调用方把 `MobileCore` 自持的那一份端口传进来 —— 不在这里另建一个同款实例。
pub(crate) async fn reconcile_stale_sessions(
    store: Arc<dyn TransferStore>,
    event_bus: Arc<dyn EventBus>,
    file_access: &Arc<dyn FileAccess>,
) -> FfiResult<usize> {
    let converted = TransferCoordinator::new(
        store.clone(),
        Arc::new(swarmdrop_core::event_adapter::CoreTransferEvents(event_bus)),
    )
    .cleanup_recoverable_sessions()
    .await
    .map_err(FfiError::from)?;

    let reaped = store
        .reap_expired_suspended_receives(swarmdrop_core::transfer::SUSPENDED_RECEIVE_RETENTION_SECS)
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
        let manager = self.transfer_manager_arc().await?;
        let items = manager
            .store()
            .list_transfer_projections()
            .await
            .map_err(FfiError::from)?;
        Ok(items.into_iter().map(Into::into).collect())
    }

    pub async fn get_transfer_projection(
        &self,
        session_id: String,
    ) -> FfiResult<Option<MobileTransferProjection>> {
        let session_uuid = parse_session_id(&session_id)?;
        let manager = self.transfer_manager_arc().await?;
        let item = manager
            .store()
            .get_transfer_projection(session_uuid)
            .await
            .map_err(FfiError::from)?;
        Ok(item.map(Into::into))
    }

    pub async fn delete_transfer_record(&self, session_id: String) -> FfiResult<()> {
        let session_uuid = parse_session_id(&session_id)?;
        // 域方法而非 store()：进行中的会话不可删是域不变量，守卫在 TransferManager。
        let manager = self.transfer_manager_arc().await?;
        manager
            .delete_session(session_uuid)
            .await
            .map_err(FfiError::from)
    }

    pub async fn clear_transfer_activity(&self) -> FfiResult<()> {
        let manager = self.transfer_manager_arc().await?;
        manager
            .store()
            .clear_all_history()
            .await
            .map_err(FfiError::from)
    }

    /// 「从历史重新发送」重建载荷用：取会话内有源路径的文件绝对路径（发送方向）。
    ///
    /// 接收会话与没记源路径的历史会话返回空 Vec —— 前端据此回退到「预选设备后重新
    /// 挑文件」，而不是假装能一键重发（源路径可能已失效，见 `[sessionId].tsx`）。
    pub async fn get_transfer_source_paths(&self, session_id: String) -> FfiResult<Vec<String>> {
        let session_uuid = parse_session_id(&session_id)?;
        let manager = self.transfer_manager_arc().await?;
        manager
            .store()
            .get_session_source_paths(session_uuid)
            .await
            .map_err(FfiError::from)
    }

    pub async fn resume_transfer(&self, session_id: String) -> FfiResult<MobileTransferProjection> {
        let session_uuid = parse_session_id(&session_id)?;
        // initiate_resume 已统一收发双向（内部按 session.direction 派生）+ 不存在 / 不可恢复
        // 校验（load_resumable_session），无需在此预加载 session 仅为取 direction。
        let manager = self.transfer_manager_arc().await?;
        manager
            .initiate_resume(session_uuid)
            .await
            .map_err(FfiError::from)?;

        let projection = manager
            .store()
            .get_transfer_projection(session_uuid)
            .await
            .map_err(FfiError::from)?
            .ok_or_else(|| FfiError::Transfer("会话不存在".into()))?;
        Ok(projection.into())
    }
}
