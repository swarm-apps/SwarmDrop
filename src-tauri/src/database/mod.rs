//! 数据库初始化模块
//!
//! 在 Tauri setup() 中初始化 SeaORM DatabaseConnection（SQLite），
//! 执行 migration，返回连接供注入 Tauri managed state。

pub use swarmdrop_core::database::ops;

use entity::{SessionStatus, TransferDirection};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Database, DatabaseConnection, EntityTrait, IntoActiveModel,
    QueryFilter, Set,
};
use sea_orm_migration::MigratorTrait;
use tauri::{AppHandle, Manager};

use crate::AppResult;

/// 初始化数据库：创建 SQLite 文件、执行 migration、返回连接
pub async fn init_database(app: &AppHandle) -> AppResult<DatabaseConnection> {
    let data_dir = app.path().app_local_data_dir()?;
    std::fs::create_dir_all(&data_dir)?;

    let db_path = data_dir.join("swarmdrop.db");
    let db_url = format!("sqlite:{}?mode=rwc", db_path.display());

    tracing::info!("初始化数据库: {}", db_url);

    let db = Database::connect(&db_url).await?;

    // 执行所有待处理的 migration
    migration::Migrator::up(&db, None).await?;

    tracing::info!("数据库 migration 完成");

    Ok(db)
}

/// 7 天过期阈值（毫秒）
const EXPIRE_DAYS_MS: i64 = 7 * 24 * 60 * 60 * 1000;

/// 更新 session 状态的辅助函数，消除重复的 into_active_model + set + update 模式
async fn finish_session(
    db: &DatabaseConnection,
    session: entity::transfer_session::Model,
    status: SessionStatus,
    error_message: Option<&str>,
) -> AppResult<()> {
    let now = ops::now_ms();
    let mut model = session.into_active_model();
    model.status = Set(status);
    model.updated_at = Set(now);
    if let Some(msg) = error_message {
        model.error_message = Set(Some(msg.into()));
    }
    // Paused 状态不设 finished_at（后续可恢复）
    if *model.status.as_ref() != SessionStatus::Paused {
        model.finished_at = Set(Some(now));
    }
    model.update(db).await?;
    Ok(())
}

/// 判断 receiver 的文件列表应转换到哪种 session 状态
fn classify_receiver_session(
    files: &[entity::transfer_file::Model],
) -> (SessionStatus, Option<&'static str>) {
    let all_completed = !files.is_empty()
        && files
            .iter()
            .all(|f| f.status == entity::FileStatus::Completed);
    if all_completed {
        return (SessionStatus::Completed, None);
    }

    let has_progress = files
        .iter()
        .any(|f| f.completed_chunks.iter().any(|&b| b != 0));
    if has_progress {
        (SessionStatus::Paused, None)
    } else {
        (SessionStatus::Failed, Some("应用重启，传输未开始"))
    }
}

/// 启动时清理中断的传输会话
///
/// - sender + transferring → failed（发送方重启，连接断开）
/// - receiver + transferring → 根据 bitmap 判断：
///   - 所有文件已完成 → completed
///   - 有进度（bitmap 非全零）→ paused
///   - 无进度（bitmap 全零）→ failed
/// - receiver + paused 超过 7 天 → failed
pub async fn cleanup_stale_sessions(db: &DatabaseConnection) -> AppResult<()> {
    use entity::transfer_session::Column;

    // 1) sender + transferring → failed
    let sender_sessions = entity::TransferSession::find()
        .filter(Column::Direction.eq(TransferDirection::Send))
        .filter(Column::Status.eq(SessionStatus::Transferring))
        .all(db)
        .await?;

    for session in sender_sessions {
        tracing::info!(
            "启动清理: sender session {} → failed（应用重启）",
            session.session_id
        );
        finish_session(
            db,
            session,
            SessionStatus::Failed,
            Some("应用重启，连接已断"),
        )
        .await?;
    }

    // 2) receiver + transferring → 根据 bitmap 判断
    let receiver_sessions = entity::TransferSession::find()
        .filter(Column::Direction.eq(TransferDirection::Receive))
        .filter(Column::Status.eq(SessionStatus::Transferring))
        .all(db)
        .await?;

    for session in receiver_sessions {
        let files = entity::TransferFile::find()
            .filter(entity::transfer_file::Column::SessionId.eq(session.session_id))
            .all(db)
            .await?;

        let (status, error_msg) = classify_receiver_session(&files);
        tracing::info!(
            "启动清理: receiver session {} → {status:?}",
            session.session_id
        );
        finish_session(db, session, status, error_msg).await?;
    }

    // 3) receiver + paused 超过 7 天 → failed
    let expired_threshold = ops::now_ms() - EXPIRE_DAYS_MS;
    let expired_sessions = entity::TransferSession::find()
        .filter(Column::Direction.eq(TransferDirection::Receive))
        .filter(Column::Status.eq(SessionStatus::Paused))
        .filter(Column::UpdatedAt.lt(expired_threshold))
        .all(db)
        .await?;

    for session in expired_sessions {
        tracing::info!(
            "启动清理: receiver session {} → failed（paused 超过 7 天）",
            session.session_id
        );

        // 清空文件 bitmap + 删除 .part 临时文件
        let files = entity::TransferFile::find()
            .filter(entity::transfer_file::Column::SessionId.eq(session.session_id))
            .all(db)
            .await?;

        for file in files {
            if let Some(entity::SaveLocation::Path { ref path }) = session.save_path {
                let final_path = std::path::Path::new(path).join(&file.relative_path);
                let part_path = crate::file_sink::compute_part_path(&final_path);
                if let Err(e) = tokio::fs::remove_file(&part_path).await {
                    if e.kind() != std::io::ErrorKind::NotFound {
                        tracing::warn!("清理 .part 文件失败（已忽略）: {e}");
                    }
                }
            }

            let mut fmodel = file.into_active_model();
            fmodel.completed_chunks = Set(vec![]);
            fmodel.transferred_bytes = Set(0);
            fmodel.status = Set(entity::FileStatus::Failed);
            fmodel.update(db).await?;
        }

        finish_session(
            db,
            session,
            SessionStatus::Failed,
            Some("传输已过期（超过 7 天）"),
        )
        .await?;
    }

    tracing::info!("启动会话清理完成");
    Ok(())
}
