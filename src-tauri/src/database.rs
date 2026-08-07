//! 数据库桥：连接初始化 + 传输域持久化端口的类型别名 + 启动清理编排。
//!
//! 在 Tauri setup() 中初始化 SeaORM DatabaseConnection（SQLite）并执行 migration；
//! 启动清理接收组装点建好的 [`TransferStoreState`] 做过期会话回收。
//!
//! **传输与收件箱的读写都不在这里** —— 一条不剩地走 [`TransferStoreState`] 端口
//! （传输侧另有 `TransferManager::store()` 出口），宿主不再另存一份 ORM 连接做业务查询。
//! `DatabaseConnection` 仍托管在 Tauri state 上，但只服务于尚未端口化的用例（邀请注册表）。

use std::sync::Arc;

use sea_orm::DatabaseConnection;
use swarmdrop_core::event_adapter::CoreTransferEvents;
use swarmdrop_core::host::{CoreSaveLocation, EventBus};
use swarmdrop_core::transfer::SUSPENDED_RECEIVE_RETENTION_SECS;
use swarmdrop_core::transfer::coordinator::TransferCoordinator;
use swarmdrop_core::transfer::store::TransferStore;
use tauri::AppHandle;

use crate::AppResult;

/// 传输域持久化端口的托管形态（会话 + 收件箱）。
///
/// 命令签名里写 `State<'_, TransferStoreState>` 而不是裸 `Arc<dyn TransferStore>`，
/// 一是可读，二是让「宿主只认端口、不认 ORM 连接」这件事在类型名上就成立。
pub type TransferStoreState = Arc<dyn TransferStore>;

/// 初始化数据库：创建 SQLite 文件、执行 migration、返回连接
pub async fn init_database(app: &AppHandle) -> AppResult<DatabaseConnection> {
    let data_dir = crate::host::paths::app_local_data_dir(app)?;
    std::fs::create_dir_all(&data_dir)?;

    let db_path = data_dir.join("swarmdrop.db");
    tracing::info!("初始化数据库: {}", db_path.display());

    // 连接 + 迁移 + 「迁移历史过时就删库重建」的自愈都在 core 的编排里（移动端共用同一条），
    // 见 `migration::connect_and_migrate`。2026-08-05 的迁移 squash 让所有存量库都会走一次
    // 那条重建路径。
    let db = migration::connect_and_migrate(&db_path).await?;

    tracing::info!("数据库 migration 完成");

    Ok(db)
}

/// 启动时清理中断的传输会话
///
/// - phase=active → 交给 core Coordinator 转 recoverable suspended(app_restarted)
/// - recoverable suspended 接收会话超过保留期未恢复 → 由共享 core 原语转 terminal，
///   再按本端真实路径尽力清理遗留 `.part`
///
/// store 由组装点（`setup.rs`）传入而非在此新建：全进程只有一份端口实例（design D5）。
pub async fn cleanup_stale_sessions(
    store: &TransferStoreState,
    event_bus: Arc<dyn EventBus>,
) -> AppResult<()> {
    let coordinator =
        TransferCoordinator::new(store.clone(), Arc::new(CoreTransferEvents(event_bus)));
    let converted = coordinator.cleanup_recoverable_sessions().await?;
    tracing::info!("启动清理: {converted} 个 active session 转为 suspended(app_restarted)");

    // 过期回收（DB 判定 + 转 terminal）走同一个 store 端口，两端一致；返回的文件元数据
    // 由桌面端按真实路径删除遗留 .part（直接 fs，不经 FileAccess 的 create-then-delete）。
    // 必须排在 cleanup_recoverable_sessions 之后：被强杀留下的 active 会话要先转 suspended
    // 才落进回收判据。
    let reaped = store
        .reap_expired_suspended_receives(SUSPENDED_RECEIVE_RETENTION_SECS)
        .await?;
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

    use entity::{SuspendedReason, TransferDirection, TransferPhase};
    // 生产路径经 `migration::connect_and_migrate` 建库（它自带自愈），测试要的是一个
    // 干净的内存库，所以这两个只在测试里用得到。
    use sea_orm::{ActiveModelTrait, ConnectOptions, Database, IntoActiveModel, Set};
    use sea_orm_migration::MigratorTrait;
    use swarmdrop_core::host::{CoreSaveLocation, MemoryHost};
    use swarmdrop_core::transfer::coordinator::TransferState;
    use swarmdrop_core::transfer::store::CreateSessionInput;
    use swarmdrop_storage_sql::SqlSessionStore;
    use uuid::Uuid;

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

    fn store_of(db: &DatabaseConnection) -> TransferStoreState {
        Arc::new(SqlSessionStore::new(Arc::new(db.clone())))
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

    /// 建会话时一次写到目标 phase。状态持久化的唯一正路是 Coordinator，
    /// 只需要「处于某 phase」的 fixture 就走 `lifecycle` 入参，不另开直写旁路。
    fn suspended_recoverable() -> TransferState {
        TransferState {
            phase: TransferPhase::Suspended,
            suspended_reason: Some(SuspendedReason::LocalPaused),
            terminal_reason: None,
            epoch: 0,
            recoverable: true,
            failure: None,
        }
    }

    /// 建一个接收会话，落到指定 lifecycle。
    async fn seed_receive_session(
        store: &TransferStoreState,
        session_id: Uuid,
        save_dir: &str,
        lifecycle: TransferState,
    ) {
        store
            .create_session(CreateSessionInput {
                session_id,
                direction: TransferDirection::Receive,
                peer_id: "peer",
                peer_name: "peer",
                files: &[test_file()],
                total_size: 16,
                save_path: Some(CoreSaveLocation::Path {
                    path: save_dir.to_string(),
                }),
                source_paths: None,
                lifecycle,
                policy: None,
                origin: None,
            })
            .await
            .expect("create receive session");
    }

    #[test]
    fn cleanup_active_sessions_uses_coordinator_app_restarted() {
        tauri::async_runtime::block_on(async {
            let db = make_db().await;
            let store = store_of(&db);
            let session_id = Uuid::new_v4();
            seed_receive_session(&store, session_id, "/recv", TransferState::active(0)).await;

            let host = MemoryHost::new();
            cleanup_stale_sessions(&store, Arc::new(host.clone()))
                .await
                .expect("cleanup");

            let projection = store
                .get_transfer_projection(session_id)
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
            let store = store_of(&db);
            let session_id = Uuid::new_v4();
            let dir = std::env::temp_dir().join(format!("swarmdrop-cleanup-{session_id}"));
            tokio::fs::create_dir_all(&dir).await.unwrap();
            let final_path = dir.join("old.bin");
            let part_path = crate::host::file_sink::compute_part_path(&final_path);
            tokio::fs::write(&part_path, b"partial").await.unwrap();

            seed_receive_session(
                &store,
                session_id,
                &dir.to_string_lossy(),
                suspended_recoverable(),
            )
            .await;

            // 把 updated_at 推回保留期之外，让它落进过期回收判据。
            let session = store.find_session(session_id).await.unwrap().unwrap();
            let mut model = session.into_active_model();
            model.updated_at = Set(chrono::Utc::now().timestamp_millis()
                - (SUSPENDED_RECEIVE_RETENTION_SECS as i64) * 1000
                - 1);
            model.update(&db).await.unwrap();

            cleanup_stale_sessions(&store, Arc::new(MemoryHost::new()))
                .await
                .expect("cleanup");

            assert!(
                !part_path.exists(),
                "过期 suspended receiver 的 .part 文件应被清理"
            );
            let projection = store
                .get_transfer_projection(session_id)
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
