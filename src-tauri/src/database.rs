//! 数据库初始化模块
//!
//! 在 Tauri setup() 中初始化 SeaORM DatabaseConnection（SQLite），
//! 执行 migration，返回连接供注入 Tauri managed state。

pub use swarmdrop_core::database::ops;

use std::sync::Arc;

use entity::{TransferDirection, TransferPhase};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Database, DatabaseConnection, EntityTrait, IntoActiveModel,
    QueryFilter, Set,
};
use sea_orm_migration::MigratorTrait;
use swarmdrop_core::host::EventBus;
use swarmdrop_core::transfer::coordinator::TransferCoordinator;
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

/// 启动时清理中断的传输会话
///
/// - phase=active → 交给 core Coordinator 转 recoverable suspended(app_restarted)
/// - receiver + suspended 超过 7 天 → failed，并清理 `.part`
pub async fn cleanup_stale_sessions(
    db: &DatabaseConnection,
    event_bus: Arc<dyn EventBus>,
) -> AppResult<()> {
    use entity::transfer_session::Column;

    let coordinator = TransferCoordinator::new(Arc::new(db.clone()), event_bus);
    let converted = coordinator.cleanup_recoverable_sessions().await?;
    tracing::info!("启动清理: {converted} 个 active session 转为 suspended(app_restarted)");

    // receiver + suspended 超过 7 天 → failed，并清理过期 .part
    let expired_threshold = ops::now_ms() - EXPIRE_DAYS_MS;
    let expired_sessions = entity::TransferSession::find()
        .filter(Column::Direction.eq(TransferDirection::Receive))
        .filter(Column::Phase.eq(TransferPhase::Suspended))
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
                let part_path = crate::host::file_sink::compute_part_path(&final_path);
                if let Err(e) = tokio::fs::remove_file(&part_path).await
                    && e.kind() != std::io::ErrorKind::NotFound
                {
                    tracing::warn!("清理 .part 文件失败（已忽略）: {e}");
                }
            }

            let mut fmodel = file.into_active_model();
            fmodel.completed_chunks = Set(vec![]);
            fmodel.transferred_bytes = Set(0);
            fmodel.status = Set(entity::FileStatus::Failed);
            fmodel.update(db).await?;
        }

        ops::mark_session_failed(db, session.session_id, "传输已过期（超过 7 天）").await?;
    }

    tracing::info!("启动会话清理完成");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use sea_orm::{ConnectOptions, EntityTrait};
    use swarmdrop_core::host::{CoreAppPaths, CoreSaveLocation, MemoryHost};
    use swarmdrop_core::transfer::coordinator::TransferState;
    use uuid::Uuid;

    fn test_paths() -> CoreAppPaths {
        let base = std::env::temp_dir();
        CoreAppPaths {
            data_dir: base.clone(),
            cache_dir: base.clone(),
            temp_dir: base.clone(),
            log_dir: base,
        }
    }

    async fn make_db() -> DatabaseConnection {
        let mut opt = ConnectOptions::new("sqlite::memory:");
        opt.max_connections(1)
            .min_connections(1)
            .sqlx_logging(false);
        let db = Database::connect(opt).await.expect("connect sqlite memory");
        migration::Migrator::up(&db, None)
            .await
            .expect("run migrations");
        db
    }

    fn test_file() -> swarmdrop_core::protocol::FileInfo {
        swarmdrop_core::protocol::FileInfo {
            file_id: 0,
            name: "old.bin".to_string(),
            relative_path: "old.bin".to_string(),
            size: 16,
            checksum: "checksum".to_string(),
        }
    }

    #[test]
    fn cleanup_active_sessions_uses_coordinator_app_restarted() {
        tauri::async_runtime::block_on(async {
            let db = make_db().await;
            let session_id = Uuid::new_v4();
            ops::create_session(
                &db,
                ops::CreateSessionInput {
                    session_id,
                    direction: TransferDirection::Receive,
                    peer_id: "peer",
                    peer_name: "peer",
                    files: &[test_file()],
                    total_size: 16,
                    save_path: Some(CoreSaveLocation::Path {
                        path: "/recv".to_string(),
                    }),
                    source_paths: None,
                    lifecycle: TransferState::active(0),
                },
            )
            .await
            .expect("create active session");

            let host = MemoryHost::new(test_paths());
            cleanup_stale_sessions(&db, Arc::new(host.clone()))
                .await
                .expect("cleanup");

            let projection = ops::get_transfer_projection(&db, session_id)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(projection.phase, entity::TransferPhase::Suspended);
            assert_eq!(
                projection.suspended_reason,
                Some(entity::SuspendedReason::AppRestarted)
            );
            assert!(projection.recoverable);
            assert!(host.events().iter().any(|e| {
                matches!(
                    e,
                    swarmdrop_core::host::CoreEvent::TransferProjection { projection }
                        if projection.session_id == session_id
                )
            }));
        });
    }

    #[test]
    fn cleanup_expired_receiver_suspended_removes_part_file_and_fails_session() {
        tauri::async_runtime::block_on(async {
            let db = make_db().await;
            let session_id = Uuid::new_v4();
            let dir = std::env::temp_dir().join(format!("swarmdrop-cleanup-{session_id}"));
            tokio::fs::create_dir_all(&dir).await.unwrap();
            let final_path = dir.join("old.bin");
            let part_path = crate::host::file_sink::compute_part_path(&final_path);
            tokio::fs::write(&part_path, b"partial").await.unwrap();

            ops::create_session(
                &db,
                ops::CreateSessionInput {
                    session_id,
                    direction: TransferDirection::Receive,
                    peer_id: "peer",
                    peer_name: "peer",
                    files: &[test_file()],
                    total_size: 16,
                    save_path: Some(CoreSaveLocation::Path {
                        path: dir.to_string_lossy().to_string(),
                    }),
                    source_paths: None,
                    lifecycle: TransferState::active(0),
                },
            )
            .await
            .expect("create receive session");
            ops::mark_session_paused(&db, session_id)
                .await
                .expect("mark paused");

            let session = entity::TransferSession::find_by_id(session_id)
                .one(&db)
                .await
                .unwrap()
                .unwrap();
            let mut model = session.into_active_model();
            model.updated_at = Set(ops::now_ms() - EXPIRE_DAYS_MS - 1);
            model.update(&db).await.unwrap();

            cleanup_stale_sessions(&db, Arc::new(MemoryHost::new(test_paths())))
                .await
                .expect("cleanup");

            assert!(
                !part_path.exists(),
                "过期 suspended receiver 的 .part 文件应被清理"
            );
            let projection = ops::get_transfer_projection(&db, session_id)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(projection.phase, entity::TransferPhase::Terminal);
            assert_eq!(
                projection.terminal_reason,
                Some(entity::TerminalReason::FatalError)
            );
            let _ = tokio::fs::remove_dir_all(&dir).await;
        });
    }
}
