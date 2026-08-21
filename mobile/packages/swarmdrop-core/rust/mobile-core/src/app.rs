//! `MobileCore` —— RN 持有的主对象。
//!
//! - constructor 是 sync(Rust `pub fn new`),RN 侧直接 `new MobileCore(keychain, eventBus, fileAccess, dataDir)`
//! - 其他业务方法分散在 identity / network / device / pairing / transfer 各模块,
//!   通过 `impl MobileCore` 跨模块挂接(Rust 允许多个 impl 块,ubrn proc-macro 都能扫到)
//! - 私有 fields 用 `pub(crate)` 访问器暴露给同 crate 内的业务模块,不暴露给外部

use std::path::{Path, PathBuf};
use std::sync::Arc;

use sea_orm::DatabaseConnection;
use swarmdrop_core::host::{
    DeviceConfig, EventBus, FileAccess, KeychainProvider, PairedDeviceStore,
};
use swarmdrop_core::network::NetManager;
use swarmdrop_core::pairing::manager::PairingManager;
use swarmdrop_core::transfer::manager::TransferManager;
use swarmdrop_core::transfer::store::TransferStore;
use swarmdrop_host_fs::JsonFileDeviceConfig;
use swarmdrop_net::{SecretKey, WebTransportConfig, WebTransportFileCertificateStore};
use tokio::sync::{Mutex, MutexGuard};

use crate::device_config::device_config_path;
use crate::error::{FfiError, FfiResult};
use crate::events::{ForeignEventBus, MobileEventBusAdapter};
use crate::file_access::{ForeignFileAccess, MobileFileAccessAdapter};
use crate::keychain::{ForeignKeychainProvider, MobileKeychainAdapter};

#[derive(uniffi::Object)]
pub struct MobileCore {
    keychain: Arc<MobileKeychainAdapter>,
    event_bus: Arc<MobileEventBusAdapter>,
    file_access: Arc<MobileFileAccessAdapter>,
    /// SQLite 文件所在目录（启动时初始化 DB 用）。
    ///
    /// 存 [`PathBuf`] 而非 `String`：宿主传进来的是 `file://` URI，在
    /// [`MobileCore::new`] 的边界就已解析完毕，此后内部一律按路径处理。
    data_dir: PathBuf,
    /// 设备名持久化端口，落在同一个 `data_dir` 下的 `device_config.json`
    device_config: Arc<JsonFileDeviceConfig>,
    /// WebTransport 证书对的持久化端口，落在同一个 `data_dir` 下的
    /// `webtransport-cert.pem`。**给了它，移动端才会监听 WebTransport。**
    ///
    /// 实现是 `crates/net` 里与桌面共用的那份（原子写 + `0600` + 读失败不降级）；
    /// 这里只决定落点，与 `device_config` 同一体例。
    webtransport_cert: Arc<WebTransportFileCertificateStore>,
    keypair: Mutex<Option<SecretKey>>,
    /// 持有 TransferManager generic 的 NetManager
    net_manager: Mutex<Option<NetManager<TransferManager>>>,
    /// SeaORM 连接 + 建立其上的持久化端口，懒初始化（首次 start_node 或收件箱查询时打开）
    db: Mutex<Option<MobileDb>>,
}

/// 已打开的 SQLite 连接与建立在它之上的传输/收件箱持久化端口。
///
/// 两者放同一个 `Option` 而不是两个字段：`store` 就是这条连接的端口视图，
/// 拆成两把锁就只能靠注释维持「同时有、同时无」。
#[derive(Clone)]
struct MobileDb {
    connection: Arc<DatabaseConnection>,
    store: Arc<dyn TransferStore>,
}

#[uniffi::export]
impl MobileCore {
    /// `data_dir` 是 host 提供的 SQLite 文件父目录（RN 传 `Paths.document.uri`，
    /// 即 `file:///...`）。它在这里被解析成 [`PathBuf`] 一次，之后内部不再有
    /// 「这个字符串是 URI 还是路径」的歧义。
    #[uniffi::constructor]
    pub fn new(
        keychain: Arc<dyn ForeignKeychainProvider>,
        event_bus: Arc<dyn ForeignEventBus>,
        file_access: Arc<dyn ForeignFileAccess>,
        data_dir: String,
    ) -> Arc<Self> {
        // 进程级 panic hook —— 只装一次,后续可用 take_last_panic() 取详情
        crate::panic_hook::install();
        // 事件总线只转发,不再持有 PairedDeviceStore —— 新配对与 Identify 刷新的写回
        // 已收进 core 的 `PairingManager::commit_paired_device`(三端同一个入口)。
        let keychain = Arc::new(MobileKeychainAdapter::new(keychain));
        // 边界解析：跨 FFI 进来的 String 在此转成可信的 PathBuf，下游不再各剥一遍前缀。
        let data_dir = crate::utils::parse_host_dir(&data_dir);
        Arc::new(Self {
            event_bus: Arc::new(MobileEventBusAdapter::new(event_bus)),
            keychain,
            // 接收暂存区挂在 data_dir 下（与 SQLite 同级），故必须在 data_dir 解析之后建。
            // 同一个 data_dir 还被用来反推**应用沙箱容器根**——发送侧读源的快路径以它
            // 为归属白名单（判定见 `file_access::route_source`）。认不出形态时快路径
            // 整个关掉，退回 JS，不影响正确性。
            file_access: Arc::new(MobileFileAccessAdapter::new(file_access, &data_dir)),
            device_config: Arc::new(JsonFileDeviceConfig::new(device_config_path(&data_dir))),
            webtransport_cert: Arc::new(WebTransportFileCertificateStore::new(
                crate::webtransport_cert::cert_path(&data_dir),
            )),
            data_dir,
            keypair: Mutex::new(None),
            net_manager: Mutex::new(None),
            db: Mutex::new(None),
        })
    }

    /// 取出最近一次 Rust panic 的详情(location + payload + 可选 backtrace)。
    /// 取过即清空 —— RN 端在 catch 到 uniffi `Rust panic` 错误后立即调一次,
    /// 把内容打到 console 便于定位。无 panic 时返回 None。
    pub fn take_last_panic(&self) -> Option<String> {
        crate::panic_hook::take_last()
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

    /// 已配对设备列表端口 —— 与 [`Self::keychain`] 同一个适配器的另一个 impl。
    /// 列表算法都在 `swarmdrop_core::paired_devices`,这里只交出端口。
    pub(crate) fn paired_device_store(&self) -> &dyn PairedDeviceStore {
        self.keychain.as_ref()
    }

    /// 同上,`start_node` 这类要长期持有端口的调用方用 Arc 版本。
    pub(crate) fn paired_device_store_arc(&self) -> Arc<dyn PairedDeviceStore> {
        self.keychain.clone()
    }

    /// 设备名持久化端口 —— `get_device_name` / `rename_device` 两个导出直接用它。
    pub(crate) fn device_config(&self) -> &dyn DeviceConfig {
        self.device_config.as_ref()
    }

    /// 同上，`start_node` 这类要长期持有端口的调用方用 Arc 版本。
    pub(crate) fn device_config_arc(&self) -> Arc<dyn DeviceConfig> {
        self.device_config.clone()
    }

    /// WebTransport 配置（带证书持久化 ⇒ 移动端监听 WebTransport）。
    ///
    /// 「有证书 ⇒ 也监听」由内核的 `bind` 兑现，这里只交出配置。
    pub(crate) fn webtransport_config(&self) -> WebTransportConfig {
        WebTransportConfig::with_store(self.webtransport_cert.clone())
    }

    /// 事件总线端口 —— 只在一次调用里用的场合取这个引用；
    /// `start_node` 那种要长期持有的用 [`Self::event_bus_arc`]。
    pub(crate) fn event_bus(&self) -> &dyn EventBus {
        self.event_bus.as_ref()
    }

    /// 传输域事件端口。
    ///
    /// 收件箱的归档与删除是**编排函数**，要往它发领域事件（spec: `inbox-domain-events`）。
    /// 包的是同一个 `event_bus`，所以节点在不在跑都到得了 JS 侧——收件箱命令刻意不依赖
    /// 节点启动。
    pub(crate) fn transfer_events(
        &self,
    ) -> Arc<dyn swarmdrop_core::transfer::events::TransferEventSink> {
        Arc::new(swarmdrop_core::event_adapter::CoreTransferEvents(
            self.event_bus_arc(),
        ))
    }

    pub(crate) fn event_bus_arc(&self) -> Arc<MobileEventBusAdapter> {
        self.event_bus.clone()
    }

    pub(crate) fn file_access_arc(&self) -> Arc<dyn FileAccess> {
        self.file_access.clone()
    }

    pub(crate) async fn set_keypair(&self, keypair: SecretKey) {
        *self.keypair.lock().await = Some(keypair);
    }

    pub(crate) async fn ensure_keypair(&self) -> FfiResult<SecretKey> {
        if let Some(keypair) = self.keypair.lock().await.as_ref().cloned() {
            return Ok(keypair);
        }
        let identity = swarmdrop_core::identity::load_or_create_identity(self.keychain())
            .await
            .map_err(FfiError::from)?;
        let keypair = identity.secret_key;
        *self.keypair.lock().await = Some(keypair.clone());
        Ok(keypair)
    }

    pub(crate) async fn net_manager_guard(
        &self,
    ) -> MutexGuard<'_, Option<NetManager<TransferManager>>> {
        self.net_manager.lock().await
    }

    pub(crate) async fn set_net_manager(&self, manager: NetManager<TransferManager>) {
        let mut guard = self.net_manager.lock().await;
        // 覆盖前先关停旧节点（如 error 态重启、shutdown 与 start 竞态后的重复启动）：
        // 否则旧 NetManager 的 cancel_token 永不触发，presence/infra 循环与
        // 旧 swarm 永久泄漏（后台持续耗电）
        if let Some(old) = guard.as_ref() {
            old.shutdown().await;
        }
        *guard = Some(manager);
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
        Ok(self.ensure_db_ready().await?.connection)
    }

    /// 传输 / 收件箱持久化端口 —— **全进程唯一的那一份**。
    ///
    /// 注入 `TransferManager` 的与 `MobileCore` 自持的是**同一个 `Arc`**，
    /// 不是两个各自包装同一条连接的实例：只有这样「宿主经端口读收件箱」与
    /// 「传输域经端口写收件箱」才是同一个对象上的两种用法，而不是两条平行的路。
    /// 顺带的收益是收件箱查询不依赖节点是否启动 —— 它按定义就是与网络无关的内容账本。
    pub(crate) async fn ensure_store(&self) -> FfiResult<Arc<dyn TransferStore>> {
        Ok(self.ensure_db_ready().await?.store)
    }

    /// 整段持锁打开数据库：check-then-act 会让并发首调各开一次库、各建一个 store，
    /// 正是上面那条纪律要杜绝的分叉。
    async fn ensure_db_ready(&self) -> FfiResult<MobileDb> {
        let mut guard = self.db.lock().await;
        if let Some(ready) = guard.as_ref() {
            return Ok(ready.clone());
        }
        let connection = Arc::new(open_db(&self.data_dir).await?);
        let ready = MobileDb {
            store: Arc::new(swarmdrop_storage_sql::SqlSessionStore::new(
                connection.clone(),
            )),
            connection,
        };
        *guard = Some(ready.clone());
        Ok(ready)
    }
}

async fn open_db(data_dir: &Path) -> FfiResult<DatabaseConnection> {
    // `data_dir` 已在 `MobileCore::new` 的边界解析过（`utils::parse_host_dir`），
    // 这里拿到的一定是文件系统路径，不必再判断格式。
    let db_path = data_dir.join("swarmdrop.db");
    tracing::info!("初始化 mobile-core 数据库: {}", db_path.display());

    // 连接 + 迁移 + 「迁移历史过时就删库重建」的自愈与桌面共用同一条编排，
    // 见 `migration::connect_and_migrate`。
    migration::connect_and_migrate(&db_path)
        .await
        .map_err(|e| FfiError::Database(e.to_string()))
}
