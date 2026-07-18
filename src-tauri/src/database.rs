//! 数据库桥：连接初始化 + storage-sql 命名空间 re-export + 启动清理编排。
//!
//! 在 Tauri setup() 中初始化 SeaORM DatabaseConnection（SQLite）并执行 migration；
//! `crate::database::{ops, inbox}` 经此处 re-export 指向 `swarmdrop-storage-sql`
//! （commands / MCP 的读查询消费点保持原路径）；启动时构造 SqlSessionStore 做过期会话清理。

pub use swarmdrop_storage_sql::{inbox, ops};

use std::sync::Arc;

use sea_orm::{Database, DatabaseConnection};
use sea_orm_migration::MigratorTrait;
use swarmdrop_core::event_adapter::CoreTransferEvents;
use swarmdrop_core::host::{CoreSaveLocation, EventBus};
use swarmdrop_core::transfer::SUSPENDED_RECEIVE_RETENTION_SECS;
use swarmdrop_core::transfer::coordinator::TransferCoordinator;
use swarmdrop_storage_sql::SqlSessionStore;
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

/// 启动时清理中断的传输会话
///
/// - phase=active → 交给 core Coordinator 转 recoverable suspended(app_restarted)
/// - recoverable suspended 接收会话超过保留期未恢复 → 由共享 core 原语转 terminal，
///   再按本端真实路径尽力清理遗留 `.part`
pub async fn cleanup_stale_sessions(
    db: &DatabaseConnection,
    event_bus: Arc<dyn EventBus>,
) -> AppResult<()> {
    let coordinator = TransferCoordinator::new(
        Arc::new(SqlSessionStore::new(Arc::new(db.clone()))),
        Arc::new(CoreTransferEvents(event_bus)),
    );
    let converted = coordinator.cleanup_recoverable_sessions().await?;
    tracing::info!("启动清理: {converted} 个 active session 转为 suspended(app_restarted)");

    // 过期回收（DB 判定 + 转 terminal）走共享 core 原语，两端一致；返回的文件元数据
    // 由桌面端按真实路径删除遗留 .part（直接 fs，不经 FileAccess 的 create-then-delete）。
    let reaped = ops::reap_expired_suspended_receives(db, SUSPENDED_RECEIVE_RETENTION_SECS).await?;
    for session in &reaped {
        tracing::info!(
            "启动清理: 过期 suspended 接收会话 {} 已回收",
            session.session_id
        );
        for meta in &session.files {
            if let Some(CoreSaveLocation::Path { path }) = &meta.save_dir {
                let final_path = std::path::Path::new(path).join(&meta.relative_path);
                let part_path = crate::host::file_sink::compute_part_path(&final_path);
                if let Err(e) = tokio::fs::remove_file(&part_path).await
                    && e.kind() != std::io::ErrorKind::NotFound
                {
                    tracing::warn!("清理 .part 文件失败（已忽略）: {e}");
                }
            }
        }
    }

    tracing::info!("启动会话清理完成");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use entity::TransferDirection;
    use sea_orm::{ActiveModelTrait, ConnectOptions, EntityTrait, IntoActiveModel, Set};
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
                    policy: None,
                    origin: None,
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
                    policy: None,
                    origin: None,
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
            model.updated_at =
                Set(ops::now_ms() - (SUSPENDED_RECEIVE_RETENTION_SECS as i64) * 1000 - 1);
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
