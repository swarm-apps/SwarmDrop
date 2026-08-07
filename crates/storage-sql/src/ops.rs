//! 传输会话 / 文件记录的运行时写路径（SeaORM 实现）。
//!
//! 这里只放**运行时**那一半：建会话、写 checkpoint、落 outboard、按状态机转换持久化。
//! 历史管理（列表投影 / 删除 / 清空 / 源路径 / 过期回收 / origin 标记）的实现本体在
//! [`crate::store`] 的 `impl SessionStore`——它们是端口方法，没有第二个调用方，
//! 不再多绕一层自由函数。

use entity::{FileStatus, TransferPhase};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityLoaderTrait, EntityTrait,
    IntoActiveModel, QueryFilter, QuerySelect, Set,
};
use uuid::Uuid;

use swarmdrop_host::AppResult;
use swarmdrop_host::CoreSaveLocation;
use swarmdrop_transfer::calc_total_chunks;
// 持久化 DTO / 投影类型随 transfer 域迁出；ops 的实现函数（SqlSessionStore 委托目标）
// 保留在 core。pub use 兼容再导出，保持 `database::ops::{TransferProjection, …}` 路径
// 不变（src-tauri events / mcp / commands 复用），From<ModelEx> 投影转换也随类型迁到 transfer。
pub use swarmdrop_transfer::store::{
    CreateSessionInput, ExpiredReceiverActor, TransferProjection, TransferProjectionFile,
};
use swarmdrop_transfer::store::{initial_completed_chunks, prefix_range, ranges_json};

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// 创建传输会话 + 关联的文件记录。
pub async fn create_session(
    db: &DatabaseConnection,
    input: CreateSessionInput<'_>,
) -> AppResult<()> {
    let now = now_ms();
    let CreateSessionInput {
        session_id,
        direction,
        peer_id,
        peer_name,
        files,
        total_size,
        save_path,
        source_paths,
        lifecycle,
        policy,
        origin,
    } = input;
    let (policy_action, policy_reason) = match policy {
        Some((action, reason)) => (Some(action.to_string()), Some(reason.to_string())),
        None => (None, None),
    };

    let mut session = entity::transfer_session::ActiveModel::builder()
        .set_session_id(session_id)
        .set_direction(direction.clone())
        .set_peer_id(entity::PeerId(peer_id.to_string()))
        .set_peer_name(peer_name.to_string())
        .set_total_size(total_size as i64)
        .set_transferred_bytes(0)
        .set_status(
            lifecycle
                .phase
                .legacy_status(lifecycle.terminal_reason.as_ref()),
        )
        .set_phase(lifecycle.phase.clone())
        .set_suspended_reason(lifecycle.suspended_reason.clone())
        .set_terminal_reason(lifecycle.terminal_reason.clone())
        .set_epoch(lifecycle.epoch)
        .set_recoverable(lifecycle.recoverable)
        .set_started_at(now)
        .set_updated_at(now)
        .set_save_path(save_path.map(Into::into))
        .set_policy_action(policy_action)
        .set_policy_reason(policy_reason)
        .set_origin(origin.map(|o| o.to_db_string()));

    for (idx, file) in files.iter().enumerate() {
        let total_chunks = calc_total_chunks(file.size) as i32;
        let completed_chunks = initial_completed_chunks(file.size, direction.clone());

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
                .set_completed_ranges("[]".to_string())
                .set_source_path(source_path),
        );
    }

    session.insert(db).await?;

    Ok(())
}

/// 接收方用户选择保存位置后，把 offered session 补齐为可启动接收的事实。
pub async fn update_session_save_path(
    db: &DatabaseConnection,
    session_id: Uuid,
    save_path: CoreSaveLocation,
) -> AppResult<()> {
    let Some(session) = entity::TransferSession::find_by_id(session_id)
        .one(db)
        .await?
    else {
        return Err(swarmdrop_host::AppError::SessionNotFound(
            "会话不存在".into(),
        ));
    };

    let mut model = session.into_active_model();
    model.save_path = Set(Some(save_path.into()));
    model.updated_at = Set(now_ms());
    model.update(db).await?;
    Ok(())
}

/// 更新文件 range checkpoint 和已传输字节数（新 data-channel 数据面使用）。
pub async fn update_file_checkpoint_ranges(
    db: &DatabaseConnection,
    session_id: Uuid,
    file_id: i32,
    completed_chunks: Vec<u8>,
    completed_ranges: &[(u64, u64)],
    transferred_bytes: i64,
) -> AppResult<()> {
    update_file(db, session_id, file_id, |model| {
        model.completed_chunks = Set(completed_chunks);
        model.completed_ranges = Set(ranges_json(completed_ranges));
        model.transferred_bytes = Set(transferred_bytes);
    })
    .await
}

/// 标记单个文件完成，写入完整 checkpoint 与最终落盘位置（finalize_sink 返回值）
pub async fn mark_file_completed(
    db: &DatabaseConnection,
    session_id: Uuid,
    file_id: i32,
    completed_chunks: Vec<u8>,
    transferred_bytes: i64,
    local_path: String,
    local_dir: String,
) -> AppResult<()> {
    update_file(db, session_id, file_id, |model| {
        model.status = Set(FileStatus::Completed);
        model.completed_chunks = Set(completed_chunks);
        model.transferred_bytes = Set(transferred_bytes);
        model.completed_ranges = Set(ranges_json(&prefix_range(transferred_bytes)));
        model.local_path = Set(Some(local_path));
        model.local_dir = Set(Some(local_dir));
    })
    .await
}

/// 更新发送方文件的已传输字节数（不修改 bitmap，发送方不使用 bitmap）
async fn update_sender_file_progress(
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
        .ok_or_else(|| swarmdrop_host::AppError::Transfer("文件记录不存在".into()))?;

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
        if transferred > 0
            && let Err(e) =
                update_sender_file_progress(db, session_id, file_id as i32, transferred as i64)
                    .await
        {
            tracing::warn!("保存发送方文件进度失败: file_id={}, {}", file_id, e);
        }
    }
    Ok(())
}

// ============ 查询 API ============

// ============ 生命周期投影（redesign-transfer-lifecycle）============
//
// `TransferProjection` / `TransferProjectionFile` / `content_root_of` 及 `From<ModelEx>`
// 投影转换随 transfer 域迁入 [`swarmdrop_transfer::store`]；下面的查询函数直接消费。

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
    state: &swarmdrop_transfer::coordinator::TransferState,
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
    // 仅在状态携带失败判别码时落库（FatalError 路径）；非 fatal 转换不清空既有 error_message。
    if let Some(failure) = &state.failure {
        model.error_message = Set(Some(failure.to_column()));
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

/// 启动清理用：查所有 `phase=Active` 的遗留会话 id。
///
/// 这些是上次运行被强杀 / 崩溃时停在传输中的会话，重启后应统一转为
/// recoverable suspended(AppRestarted)，由 `TransferCoordinator::cleanup_recoverable_sessions`
/// 经状态机驱动。
pub async fn find_active_session_ids(db: &DatabaseConnection) -> AppResult<Vec<Uuid>> {
    active_session_ids(db, None).await
}

/// 对端断连用：查某个 peer 当前所有 `phase=Active` 的会话 id，转为
/// recoverable suspended(Interrupted)。
pub async fn find_active_session_ids_by_peer(
    db: &DatabaseConnection,
    peer_id: &str,
) -> AppResult<Vec<Uuid>> {
    active_session_ids(db, Some(peer_id)).await
}

async fn active_session_ids(
    db: &DatabaseConnection,
    peer_id: Option<&str>,
) -> AppResult<Vec<Uuid>> {
    let mut query = entity::TransferSession::find()
        .filter(entity::transfer_session::Column::Phase.eq(TransferPhase::Active));
    if let Some(peer) = peer_id {
        query = query
            .filter(entity::transfer_session::Column::PeerId.eq(entity::PeerId(peer.to_string())));
    }
    Ok(query
        .all(db)
        .await?
        .into_iter()
        .map(|m| m.session_id)
        .collect())
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

/// 持久化发送方某文件的 bao outboard（逐块验签 Merkle 树）。
pub async fn save_file_outboard(
    db: &DatabaseConnection,
    session_id: Uuid,
    file_id: i32,
    outboard: Vec<u8>,
) -> AppResult<()> {
    update_file(db, session_id, file_id, |model| {
        model.outboard = Set(Some(outboard));
    })
    .await
}

/// 载入发送方某文件的 bao outboard；无记录返回 `None`。
///
/// 只取 `outboard` 一列并把 NULL 过滤下推到 SQL，避免物化 bitmap BLOB 等无关列。
pub async fn load_file_outboard(
    db: &DatabaseConnection,
    session_id: Uuid,
    file_id: i32,
) -> AppResult<Option<Vec<u8>>> {
    Ok(entity::TransferFile::find()
        .select_only()
        .column(entity::transfer_file::Column::Outboard)
        .filter(entity::transfer_file::Column::SessionId.eq(session_id))
        .filter(entity::transfer_file::Column::FileId.eq(file_id))
        .filter(entity::transfer_file::Column::Outboard.is_not_null())
        .into_tuple::<Vec<u8>>()
        .one(db)
        .await?)
}
