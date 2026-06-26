//! 数据库操作辅助函数
//!
//! 封装传输会话和文件记录的 CRUD 操作，供传输模块和命令层调用。

use entity::{
    FileStatus, SessionStatus, SuspendedReason, TerminalReason, TransferDirection, TransferPhase,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityLoaderTrait, EntityTrait,
    IntoActiveModel, QueryFilter, QueryOrder, Set,
};
use uuid::Uuid;

use crate::host::CoreSaveLocation;
use crate::protocol::FileInfo;
use crate::transfer::calc_total_chunks;
use crate::AppResult;

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// 创建传输会话 + 关联的文件记录
///
/// `source_paths`：发送方传入每个文件的绝对路径（与 `files` 一一对应），
/// 接收方传 `None`。用于断点续传时重建 `FileSource`。
#[expect(clippy::too_many_arguments, reason = "DB 写入需要完整上下文")]
pub async fn create_session(
    db: &DatabaseConnection,
    session_id: Uuid,
    direction: TransferDirection,
    peer_id: &str,
    peer_name: &str,
    files: &[FileInfo],
    total_size: u64,
    save_path: Option<CoreSaveLocation>,
    source_paths: Option<&[String]>,
) -> AppResult<()> {
    let now = now_ms();

    let mut session = entity::transfer_session::ActiveModel::builder()
        .set_session_id(session_id)
        .set_direction(direction.clone())
        .set_peer_id(entity::PeerId(peer_id.to_string()))
        .set_peer_name(peer_name.to_string())
        .set_total_size(total_size as i64)
        .set_transferred_bytes(0)
        .set_status(SessionStatus::Transferring)
        .set_phase(TransferPhase::Active)
        .set_epoch(0)
        .set_recoverable(true)
        .set_started_at(now)
        .set_updated_at(now)
        .set_save_path(save_path.map(Into::into));

    for (idx, file) in files.iter().enumerate() {
        let total_chunks = calc_total_chunks(file.size) as i32;
        let bitmap_len = (total_chunks as usize).div_ceil(8);
        let completed_chunks = if direction == TransferDirection::Receive {
            vec![0u8; bitmap_len]
        } else {
            vec![]
        };

        let source_path = source_paths.and_then(|paths| paths.get(idx).cloned());

        session = session.add_file(
            entity::transfer_file::ActiveModel::builder()
                .set_file_id(file.file_id as i32)
                .set_session_id(session_id)
                .set_name(file.name.clone())
                .set_relative_path(file.relative_path.clone())
                .set_size(file.size as i64)
                .set_checksum(file.checksum.clone())
                .set_status(FileStatus::Pending)
                .set_transferred_bytes(0)
                .set_total_chunks(total_chunks)
                .set_completed_chunks(completed_chunks)
                .set_source_path(source_path),
        );
    }

    session.insert(db).await?;

    Ok(())
}

/// 更新文件的 bitmap 和已传输字节数（断点续传 checkpoint）
pub async fn update_file_checkpoint(
    db: &DatabaseConnection,
    session_id: Uuid,
    file_id: i32,
    completed_chunks: Vec<u8>,
    transferred_bytes: i64,
) -> AppResult<()> {
    update_file(db, session_id, file_id, |model| {
        model.completed_chunks = Set(completed_chunks);
        model.transferred_bytes = Set(transferred_bytes);
    })
    .await
}

/// 标记单个文件完成，并写入完整 checkpoint
pub async fn mark_file_completed(
    db: &DatabaseConnection,
    session_id: Uuid,
    file_id: i32,
    completed_chunks: Vec<u8>,
    transferred_bytes: i64,
) -> AppResult<()> {
    update_file(db, session_id, file_id, |model| {
        model.status = Set(FileStatus::Completed);
        model.completed_chunks = Set(completed_chunks);
        model.transferred_bytes = Set(transferred_bytes);
    })
    .await
}

/// 重置文件的 checkpoint（bitmap 清零 + transferred_bytes 归零）
///
/// 校验失败后调用——.part 文件已被删除，需要清除 DB 中的 bitmap，
/// 确保下次恢复时重新下载该文件的所有 chunk。
pub async fn reset_file_checkpoint(
    db: &DatabaseConnection,
    session_id: Uuid,
    file_id: i32,
) -> AppResult<()> {
    update_file(db, session_id, file_id, |model| {
        model.completed_chunks = Set(vec![]);
        model.transferred_bytes = Set(0);
    })
    .await
}

/// 更新发送方文件的已传输字节数（不修改 bitmap，发送方不使用 bitmap）
pub async fn update_sender_file_progress(
    db: &DatabaseConnection,
    session_id: Uuid,
    file_id: i32,
    transferred_bytes: i64,
) -> AppResult<()> {
    update_file(db, session_id, file_id, |model| {
        model.transferred_bytes = Set(transferred_bytes);
    })
    .await
}

/// 通用文件更新：加载文件记录 → 应用修改 → 保存（同时更新关联 session 的 updated_at）
async fn update_file<F>(
    db: &DatabaseConnection,
    session_id: Uuid,
    file_id: i32,
    apply: F,
) -> AppResult<()>
where
    F: FnOnce(&mut entity::transfer_file::ActiveModelEx),
{
    let file = entity::TransferFile::load()
        .filter(entity::transfer_file::Column::SessionId.eq(session_id))
        .filter(entity::transfer_file::Column::FileId.eq(file_id))
        .with(entity::TransferSession)
        .one(db)
        .await?
        .ok_or_else(|| crate::AppError::Transfer("文件记录不存在".into()))?;

    let mut model = file.into_active_model();
    apply(&mut model);
    if let Some(session) = model.session.as_mut() {
        session.updated_at = Set(now_ms());
    }
    model.save(db).await?;

    Ok(())
}

/// 批量保存发送方 per-file 进度到 DB（断点续传恢复时使用）
///
/// `progress` 为 `(file_id, chunks_done, transferred_bytes)` 三元组列表。
pub async fn save_sender_file_progress(
    db: &DatabaseConnection,
    session_id: Uuid,
    progress: &[(u32, u32, u64)],
) -> AppResult<()> {
    for &(file_id, _chunks_done, transferred) in progress {
        if transferred > 0 {
            if let Err(e) =
                update_sender_file_progress(db, session_id, file_id as i32, transferred as i64)
                    .await
            {
                tracing::warn!("保存发送方文件进度失败: file_id={}, {}", file_id, e);
            }
        }
    }
    Ok(())
}

/// 更新 session 的已传输字节数
pub async fn update_session_transferred_bytes(
    db: &DatabaseConnection,
    session_id: Uuid,
    transferred_bytes: i64,
) -> AppResult<()> {
    if let Some(session) = entity::TransferSession::find_by_id(session_id)
        .one(db)
        .await?
    {
        let mut model = session.into_active_model();
        model.transferred_bytes = Set(transferred_bytes);
        model.updated_at = Set(now_ms());
        model.update(db).await?;
    }
    Ok(())
}

/// 从文件记录汇总已传输字节数，同步到 session 级别
pub async fn sync_session_transferred_bytes(
    db: &DatabaseConnection,
    session_id: Uuid,
) -> AppResult<()> {
    let files = get_session_files(db, session_id).await?;
    let total_transferred: i64 = files.iter().map(|f| f.transferred_bytes).sum();
    update_session_transferred_bytes(db, session_id, total_transferred).await
}

/// 过渡期桥接（反向）：旧 `mark_session_*` 写 status 时，同步写新 phase/reason/recoverable，
/// 保持 DB 两种表示一致。后续 Coordinator 接线后状态决策收归 `dispatch`，这些 `mark_*` 将被替换。
fn set_session_lifecycle(
    model: &mut entity::transfer_session::ActiveModel,
    phase: TransferPhase,
    suspended_reason: Option<SuspendedReason>,
    terminal_reason: Option<TerminalReason>,
) {
    model.recoverable = Set(!matches!(phase, TransferPhase::Terminal));
    model.phase = Set(phase);
    model.suspended_reason = Set(suspended_reason);
    model.terminal_reason = Set(terminal_reason);
}

/// 标记传输完成
pub async fn mark_session_completed(db: &DatabaseConnection, session_id: Uuid) -> AppResult<()> {
    let now = now_ms();

    entity::TransferFile::update_many()
        .col_expr(
            entity::transfer_file::Column::Status,
            sea_orm::prelude::Expr::value(FileStatus::Completed),
        )
        .filter(entity::transfer_file::Column::SessionId.eq(session_id))
        .exec(db)
        .await?;

    if let Some(session) = entity::TransferSession::find_by_id(session_id)
        .one(db)
        .await?
    {
        let mut model = session.into_active_model();
        model.status = Set(SessionStatus::Completed);
        set_session_lifecycle(
            &mut model,
            TransferPhase::Terminal,
            None,
            Some(TerminalReason::Completed),
        );
        model.transferred_bytes = Set(*model.total_size.as_ref());
        model.finished_at = Set(Some(now));
        model.updated_at = Set(now);
        model.update(db).await?;
    }

    Ok(())
}

/// 标记传输失败
pub async fn mark_session_failed(
    db: &DatabaseConnection,
    session_id: Uuid,
    error_message: &str,
) -> AppResult<()> {
    update_session_terminal(db, session_id, |model, now| {
        model.status = Set(SessionStatus::Failed);
        set_session_lifecycle(
            model,
            TransferPhase::Terminal,
            None,
            Some(TerminalReason::FatalError),
        );
        model.error_message = Set(Some(error_message.to_string()));
        model.finished_at = Set(Some(now));
        model.updated_at = Set(now);
    })
    .await
}

/// 标记传输取消
pub async fn mark_session_cancelled(db: &DatabaseConnection, session_id: Uuid) -> AppResult<()> {
    update_session_terminal(db, session_id, |model, now| {
        model.status = Set(SessionStatus::Cancelled);
        set_session_lifecycle(
            model,
            TransferPhase::Terminal,
            None,
            Some(TerminalReason::Cancelled),
        );
        model.finished_at = Set(Some(now));
        model.updated_at = Set(now);
    })
    .await
}

/// 标记传输暂停
pub async fn mark_session_paused(db: &DatabaseConnection, session_id: Uuid) -> AppResult<()> {
    update_session_terminal(db, session_id, |model, now| {
        model.status = Set(SessionStatus::Paused);
        // 过渡期默认 LocalPaused；对端暂停的精确区分留待 Coordinator 接线（NetworkSignal::RemotePaused）。
        set_session_lifecycle(
            model,
            TransferPhase::Suspended,
            Some(SuspendedReason::LocalPaused),
            None,
        );
        model.updated_at = Set(now);
    })
    .await
}

/// 标记暂停并同步 session 级别已传输字节数
pub async fn pause_session(db: &DatabaseConnection, session_id: Uuid) -> AppResult<()> {
    mark_session_paused(db, session_id).await?;
    sync_session_transferred_bytes(db, session_id).await
}

/// 恢复传输：paused/failed → transferring
pub async fn mark_session_transferring(db: &DatabaseConnection, session_id: Uuid) -> AppResult<()> {
    update_session_terminal(db, session_id, |model, now| {
        model.status = Set(SessionStatus::Transferring);
        set_session_lifecycle(model, TransferPhase::Active, None, None);
        model.updated_at = Set(now);
    })
    .await
}

/// 查找 session 并应用状态更新，不存在时静默跳过（DB 可选场景）
async fn update_session_terminal<F>(
    db: &DatabaseConnection,
    session_id: Uuid,
    apply: F,
) -> AppResult<()>
where
    F: FnOnce(&mut entity::transfer_session::ActiveModel, i64),
{
    if let Some(session) = entity::TransferSession::find_by_id(session_id)
        .one(db)
        .await?
    {
        let mut model = session.into_active_model();
        apply(&mut model, now_ms());
        model.update(db).await?;
    }
    Ok(())
}

// ============ 查询 API ============

/// 传输历史记录（session + files）
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct TransferHistoryItem {
    pub session_id: Uuid,
    pub direction: TransferDirection,
    pub peer_id: String,
    pub peer_name: String,
    pub total_size: i64,
    pub transferred_bytes: i64,
    pub status: SessionStatus,
    pub started_at: i64,
    pub updated_at: i64,
    pub finished_at: Option<i64>,
    pub error_message: Option<String>,
    pub save_path: Option<CoreSaveLocation>,
    pub files: Vec<TransferHistoryFile>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct TransferHistoryFile {
    pub file_id: i32,
    pub name: String,
    pub relative_path: String,
    pub size: i64,
    pub status: FileStatus,
    pub transferred_bytes: i64,
}

impl From<entity::transfer_file::ModelEx> for TransferHistoryFile {
    fn from(f: entity::transfer_file::ModelEx) -> Self {
        Self {
            file_id: f.file_id,
            name: f.name,
            relative_path: f.relative_path,
            size: f.size,
            status: f.status,
            transferred_bytes: f.transferred_bytes,
        }
    }
}

impl From<entity::transfer_session::ModelEx> for TransferHistoryItem {
    fn from(session: entity::transfer_session::ModelEx) -> Self {
        Self {
            session_id: session.session_id,
            direction: session.direction,
            peer_id: session.peer_id.0,
            peer_name: session.peer_name,
            total_size: session.total_size,
            transferred_bytes: session.transferred_bytes,
            status: session.status,
            started_at: session.started_at,
            updated_at: session.updated_at,
            finished_at: session.finished_at,
            error_message: session.error_message,
            save_path: session.save_path.map(Into::into),
            files: session.files.into_iter().map(Into::into).collect(),
        }
    }
}

// ============ 生命周期投影（redesign-transfer-lifecycle）============

/// 传输投影 DTO —— 前端唯一状态源（逐步替代旧的分散事件 + 扁平 `SessionStatus`）。
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct TransferProjection {
    pub session_id: Uuid,
    pub direction: TransferDirection,
    pub peer_id: String,
    pub peer_name: String,
    pub phase: TransferPhase,
    pub suspended_reason: Option<SuspendedReason>,
    pub terminal_reason: Option<TerminalReason>,
    pub recoverable: bool,
    pub epoch: i64,
    pub total_size: i64,
    pub transferred_bytes: i64,
    pub started_at: i64,
    pub updated_at: i64,
    pub finished_at: Option<i64>,
    pub error_message: Option<String>,
    pub save_path: Option<CoreSaveLocation>,
    pub files: Vec<TransferProjectionFile>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct TransferProjectionFile {
    pub file_id: i32,
    pub name: String,
    pub relative_path: String,
    pub size: i64,
    pub transferred_bytes: i64,
}

impl From<entity::transfer_file::ModelEx> for TransferProjectionFile {
    fn from(f: entity::transfer_file::ModelEx) -> Self {
        Self {
            file_id: f.file_id,
            name: f.name,
            relative_path: f.relative_path,
            size: f.size,
            transferred_bytes: f.transferred_bytes,
        }
    }
}

impl From<entity::transfer_session::ModelEx> for TransferProjection {
    fn from(s: entity::transfer_session::ModelEx) -> Self {
        Self {
            session_id: s.session_id,
            direction: s.direction,
            peer_id: s.peer_id.0,
            peer_name: s.peer_name,
            phase: s.phase,
            suspended_reason: s.suspended_reason,
            terminal_reason: s.terminal_reason,
            recoverable: s.recoverable,
            epoch: s.epoch,
            total_size: s.total_size,
            transferred_bytes: s.transferred_bytes,
            started_at: s.started_at,
            updated_at: s.updated_at,
            finished_at: s.finished_at,
            error_message: s.error_message,
            save_path: s.save_path.map(Into::into),
            files: s.files.into_iter().map(Into::into).collect(),
        }
    }
}

/// 查询所有传输投影（前端列表唯一数据源，按开始时间倒序）。
pub async fn get_transfer_projections(
    db: &DatabaseConnection,
) -> AppResult<Vec<TransferProjection>> {
    let sessions = load_sessions_with_files(db, None).await?;
    Ok(sessions.into_iter().map(Into::into).collect())
}

/// 查询单个 session 的投影（状态转换后给前端 emit，避免全表 load）。
pub async fn get_transfer_projection(
    db: &DatabaseConnection,
    session_id: Uuid,
) -> AppResult<Option<TransferProjection>> {
    let session = entity::TransferSession::load()
        .filter_by_id(session_id)
        .with(entity::TransferFile)
        .one(db)
        .await?;
    Ok(session.map(Into::into))
}

/// 按 reducer 输出的新状态写入 session 的 phase / reason / epoch / recoverable。
///
/// 这是 Coordinator 唯一的状态持久化入口（D6：DB 是恢复事实来源）。
pub async fn apply_transition(
    db: &DatabaseConnection,
    session: &entity::transfer_session::Model,
    state: &crate::transfer::coordinator::TransferState,
) -> AppResult<()> {
    let mut model = session.clone().into_active_model();
    model.phase = Set(state.phase.clone());
    model.suspended_reason = Set(state.suspended_reason.clone());
    model.terminal_reason = Set(state.terminal_reason.clone());
    model.epoch = Set(state.epoch);
    model.recoverable = Set(state.recoverable);
    // 过渡期：同步旧扁平 status（单一映射来源 TransferPhase::legacy_status），
    // 避免 phase 与 status 漂移、前端旧路径读到滞留状态。
    model.status = Set(state.phase.legacy_status(state.terminal_reason.as_ref()));
    model.updated_at = Set(now_ms());
    if state.is_terminal() {
        model.finished_at = Set(Some(now_ms()));
    }
    model.update(db).await?;
    Ok(())
}

/// 加载 session 原始 Model（供 Coordinator 读取当前生命周期状态）。
pub async fn find_session(
    db: &DatabaseConnection,
    session_id: Uuid,
) -> AppResult<Option<entity::transfer_session::Model>> {
    Ok(entity::TransferSession::find_by_id(session_id)
        .one(db)
        .await?)
}

/// 加载 session + files，按开始时间倒序（可选按旧 status 过滤）。
/// `get_transfer_history` 与 `get_transfer_projections` 共用同一查询体。
async fn load_sessions_with_files(
    db: &DatabaseConnection,
    status_filter: Option<SessionStatus>,
) -> AppResult<Vec<entity::transfer_session::ModelEx>> {
    let mut query = entity::TransferSession::load()
        .with(entity::TransferFile)
        .order_by_desc(entity::transfer_session::Column::StartedAt);
    if let Some(status) = status_filter {
        query = query.filter(entity::transfer_session::Column::Status.eq(status));
    }
    Ok(query.all(db).await?)
}

/// 查询传输历史列表（可选按状态过滤）
pub async fn get_transfer_history(
    db: &DatabaseConnection,
    status_filter: Option<SessionStatus>,
) -> AppResult<Vec<TransferHistoryItem>> {
    let sessions = load_sessions_with_files(db, status_filter).await?;
    Ok(sessions.into_iter().map(Into::into).collect())
}

/// 查询单个传输会话详情
pub async fn get_session_detail(
    db: &DatabaseConnection,
    session_id: Uuid,
) -> AppResult<TransferHistoryItem> {
    let session = entity::TransferSession::load()
        .filter_by_id(session_id)
        .with(entity::TransferFile)
        .one(db)
        .await?
        .ok_or_else(|| crate::AppError::Transfer("会话不存在".into()))?;

    Ok(session.into())
}

/// 删除单个传输会话及关联文件（级联删除）
pub async fn delete_session(db: &DatabaseConnection, session_id: Uuid) -> AppResult<()> {
    if let Some(session) = entity::TransferSession::find_by_id(session_id)
        .one(db)
        .await?
    {
        session.cascade_delete(db).await?;
    }

    Ok(())
}

/// 清空所有传输历史
pub async fn clear_all_history(db: &DatabaseConnection) -> AppResult<()> {
    entity::TransferFile::delete_many().exec(db).await?;
    entity::TransferSession::delete_many().exec(db).await?;
    Ok(())
}

/// 获取 session 的文件列表（含 bitmap，供断点续传使用）
pub async fn get_session_files(
    db: &DatabaseConnection,
    session_id: Uuid,
) -> AppResult<Vec<entity::transfer_file::Model>> {
    Ok(entity::TransferFile::find()
        .filter(entity::transfer_file::Column::SessionId.eq(session_id))
        .all(db)
        .await?)
}
