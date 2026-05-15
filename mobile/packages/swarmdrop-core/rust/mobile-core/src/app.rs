//! `MobileCore` —— RN 持有的主对象。
//!
//! - constructor 是 sync(Rust `pub fn new`),RN 侧直接 `new MobileCore(keychain, eventBus, fileAccess, dataDir)`
//! - 其他业务方法分散在 identity / network / device / pairing / transfer 各模块,
//!   通过 `impl MobileCore` 跨模块挂接(Rust 允许多个 impl 块,ubrn proc-macro 都能扫到)
//! - 私有 fields 用 `pub(crate)` 访问器暴露给同 crate 内的业务模块,不暴露给外部

use std::sync::Arc;

use sea_orm::DatabaseConnection;
use swarm_p2p_core::libp2p::identity::Keypair;
use swarmdrop_core::host::{FileAccess, KeychainProvider};
use swarmdrop_core::network::NetManager;
use swarmdrop_core::pairing::manager::PairingManager;
use swarmdrop_core::transfer::manager::TransferManager;
use tokio::sync::{Mutex, MutexGuard};

use crate::error::{FfiError, FfiResult};
use crate::events::{ForeignEventBus, MobileEventBusAdapter};
use crate::file_access::{ForeignFileAccess, MobileFileAccessAdapter};
use crate::keychain::{ForeignKeychainProvider, MobileKeychainAdapter};

#[derive(uniffi::Object)]
pub struct MobileCore {
    keychain: Arc<MobileKeychainAdapter>,
    event_bus: Arc<MobileEventBusAdapter>,
    file_access: Arc<MobileFileAccessAdapter>,
    /// SQLite 文件所在目录（启动时初始化 DB 用）
    data_dir: String,
    keypair: Mutex<Option<Keypair>>,
    /// 持有 TransferManager generic 的 NetManager
    net_manager: Mutex<Option<NetManager<TransferManager>>>,
    /// SeaORM 连接，懒初始化（首次 start_node 时打开）
    db: Mutex<Option<Arc<DatabaseConnection>>>,
}

#[uniffi::export]
impl MobileCore {
    /// `data_dir` 是 host 提供的 SQLite 文件父目录（RN 用 `Paths.document.uri`）
    #[uniffi::constructor]
    pub fn new(
        keychain: Arc<dyn ForeignKeychainProvider>,
        event_bus: Arc<dyn ForeignEventBus>,
        file_access: Arc<dyn ForeignFileAccess>,
        data_dir: String,
    ) -> Arc<Self> {
        Arc::new(Self {
            keychain: Arc::new(MobileKeychainAdapter::new(keychain)),
            event_bus: Arc::new(MobileEventBusAdapter::new(event_bus)),
            file_access: Arc::new(MobileFileAccessAdapter::new(file_access)),
            data_dir,
            keypair: Mutex::new(None),
            net_manager: Mutex::new(None),
            db: Mutex::new(None),
        })
    }
}

// ── 私有 helpers ─────────────────────────────────────────────
//
// `pub(crate)` 访问器给同 crate 内业务模块用。不加 #[uniffi::export],
// 不会出现在 TS bindings 里。

impl MobileCore {
    pub(crate) fn keychain(&self) -> &dyn KeychainProvider {
        self.keychain.as_ref()
    }

    pub(crate) fn event_bus_arc(&self) -> Arc<MobileEventBusAdapter> {
        self.event_bus.clone()
    }

    pub(crate) fn file_access_arc(&self) -> Arc<dyn FileAccess> {
        self.file_access.clone()
    }

    pub(crate) async fn set_keypair(&self, keypair: Keypair) {
        *self.keypair.lock().await = Some(keypair);
    }

    pub(crate) async fn ensure_keypair(&self) -> FfiResult<Keypair> {
        if let Some(keypair) = self.keypair.lock().await.as_ref().cloned() {
            return Ok(keypair);
        }
        let identity = swarmdrop_core::identity::load_or_create_identity(self.keychain())
            .await
            .map_err(FfiError::from)?;
        let keypair = identity.keypair;
        *self.keypair.lock().await = Some(keypair.clone());
        Ok(keypair)
    }

    pub(crate) async fn net_manager_guard(
        &self,
    ) -> MutexGuard<'_, Option<NetManager<TransferManager>>> {
        self.net_manager.lock().await
    }

    pub(crate) async fn set_net_manager(&self, manager: NetManager<TransferManager>) {
        *self.net_manager.lock().await = Some(manager);
    }

    pub(crate) async fn pairing_manager(&self) -> FfiResult<Arc<PairingManager>> {
        self.net_manager
            .lock()
            .await
            .as_ref()
            .map(|manager| manager.shared_refs().pairing)
            .ok_or(FfiError::NodeNotStarted)
    }

    pub(crate) async fn transfer_manager_arc(&self) -> FfiResult<Arc<TransferManager>> {
        self.net_manager
            .lock()
            .await
            .as_ref()
            .map(|manager| manager.transfer_arc())
            .ok_or(FfiError::NodeNotStarted)
    }

    pub(crate) async fn ensure_db(&self) -> FfiResult<Arc<DatabaseConnection>> {
        {
            let guard = self.db.lock().await;
            if let Some(db) = guard.as_ref() {
                return Ok(db.clone());
            }
        }
        let db = open_db(&self.data_dir).await?;
        let db_arc = Arc::new(db);
        *self.db.lock().await = Some(db_arc.clone());
        Ok(db_arc)
    }
}

async fn open_db(data_dir: &str) -> FfiResult<DatabaseConnection> {
    use sea_orm::Database;
    use sea_orm_migration::MigratorTrait;

    // 去掉 file:// 前缀（expo Paths.document.uri 是 file:///path/to/dir）
    let dir = data_dir
        .strip_prefix("file://")
        .unwrap_or(data_dir)
        .trim_end_matches('/');
    let db_path = format!("{dir}/swarmdrop.db");
    let db_url = format!("sqlite:{db_path}?mode=rwc");
    tracing::info!("初始化 mobile-core 数据库: {db_url}");

    let db = Database::connect(&db_url)
        .await
        .map_err(|e| FfiError::Database(e.to_string()))?;
    migration::Migrator::up(&db, None)
        .await
        .map_err(|e| FfiError::Database(e.to_string()))?;
    Ok(db)
}
