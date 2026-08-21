//! 节点装配：凑齐端口 → 调用核心的组合根。
//!
//! **本模块不含任何面向用户的文案**（见 [`crate::runtime`] 的分层约束）。它也不假设
//! 调用方是一次性命令：常驻的 `start`、一次性的 `send`、以及将来可能出现的交互式前端
//! 都用同一条装配路径，差别只在拿到 [`RunningNode`] 之后怎么用它。

use std::sync::Arc;

use swarmdrop_core::event_adapter::CoreTransferEvents;
use swarmdrop_core::host::{DeviceConfig, EventBus, FileAccess};
use swarmdrop_core::network::NetManager;
use swarmdrop_core::runtime::{EndpointProfile, HostPorts, NodeCredentials, start_node};
use swarmdrop_core::transfer::manager::TransferManager;
use swarmdrop_core::transfer::store::TransferStore;
use swarmdrop_host::device::{DeviceName, OsInfo};
use swarmdrop_host_fs::{JsonFileDeviceConfig, JsonFileIdentityStore, LocalFileAccess};
use swarmdrop_net::{WebTransportConfig, WebTransportFileCertificateStore};

use crate::adapter::events::CliEventBus;
use crate::adapter::paths::DataDir;
use crate::exit::{CliError, CliResult};

use super::bootstrap_nodes::network_config;
use super::settings::SettingsStore;

/// 命令行宿主的网络管理器类型。
///
/// **存在的理由是 `Option<&CliNetManager>` 的 `None` 分支**：核心的 `unpair` 收
/// `Option<&NetManager<T>>`，传 `None` 时 `T` 推不出来，必须显式标注。别名收在这里，
/// 免得每个「节点可能不在」的调用点各写一遍那串类型。
pub type CliNetManager = NetManager<TransferManager>;

/// 一个已经起来的节点，以及命令还用得着的东西。
///
/// **刻意不持有存储与文件访问端口**：它们已经注入 `TransferManager`，命令经它操作即可。
/// 再存一份 `Arc` 只会诱使调用方绕过管理器直接动存储——那正是「两张进行中的暂存表」
/// 这类问题的来源。收件箱命令不经节点、直接连库，也不需要它们。
pub struct RunningNode {
    pub manager: CliNetManager,
    /// 本机节点标识。
    pub node_id: swarmdrop_net::NodeId,
    /// 身份密钥。生成配对邀请要用它签名。
    pub secret_key: swarmdrop_net::SecretKey,
    /// 事件总线。一次性命令经它订阅自己关心的终态。
    pub events: Arc<CliEventBus>,
}

/// WebTransport 证书文件名（与桌面、移动同名，各端只是目录不同）。
const WEBTRANSPORT_CERT_FILE: &str = "webtransport-cert.pem";

/// 装配并启动节点。
///
/// `json` 决定事件渲染方式——它属于装配参数而非用户文案，故在这里接。
pub async fn boot(data_dir: &DataDir, json: bool) -> CliResult<RunningNode> {
    // **先读配置再动别的**：引导清单要交给组合根，而配置文件读坏时节点必须起不来。
    // 静默回落到内置清单等于让用户自己加的那条中继无声消失，故障形态是「跨网突然连不上」。
    let settings = SettingsStore::new(data_dir.settings()).read()?;

    let identity_store = Arc::new(JsonFileIdentityStore::new(data_dir.path()));

    let identity = swarmdrop_core::identity::load_or_create_identity(&*identity_store)
        .await
        .map_err(|e| CliError::NodeUnavailable(format!("读取设备身份失败: {e}")))?;

    let webrtc_certificate_pem =
        swarmdrop_core::identity::load_or_create_webrtc_certificate(&*identity_store)
            .await
            .map_err(|e| CliError::NodeUnavailable(format!("准备 WebRTC 证书失败: {e}")))?;

    let db = migration::connect_and_migrate(&data_dir.database())
        .await
        .map_err(|e| CliError::NodeUnavailable(format!("初始化数据库失败: {e}")))?;

    let store: Arc<dyn TransferStore> = Arc::new(swarmdrop_storage_sql::SqlSessionStore::new(
        Arc::new(db.clone()),
    ));
    let file_access: Arc<dyn FileAccess> = Arc::new(LocalFileAccess::new());
    let cli_events = Arc::new(CliEventBus::for_mode(json));
    let event_bus: Arc<dyn EventBus> = cli_events.clone();

    let device_config: Arc<dyn DeviceConfig> =
        Arc::new(JsonFileDeviceConfig::new(data_dir.device_config()));
    ensure_device_name(&*device_config).await;

    let webtransport = WebTransportConfig::with_store(Arc::new(
        WebTransportFileCertificateStore::new(data_dir.path().join(WEBTRANSPORT_CERT_FILE)),
    ));

    let store_for_factory = store.clone();
    let file_access_for_factory = file_access.clone();
    let event_bus_for_factory = event_bus.clone();

    let started = start_node(
        NodeCredentials {
            secret_key: identity.secret_key.clone(),
            webrtc_certificate_pem: Some(webrtc_certificate_pem),
            // 带证书持久化 ⇒ 也监听 WebTransport。判据是「宿主给没给证书端口」，
            // 不是「是不是原生端」——命令行宿主有本地文件系统，所以给得起。
            webtransport: Some(webtransport),
        },
        OsInfo::default(),
        network_config(&settings.bootstrap),
        EndpointProfile::Native,
        HostPorts {
            device_config,
            paired_device_store: identity_store.clone(),
            event_bus: event_bus.clone(),
            // 命令行没有系统通知这个概念——端口本身就是 `Option`。
            notifier: None,
            invite_store: Arc::new(swarmdrop_storage_sql::SqlInviteStore::new(db.clone())),
        },
        move |endpoint| {
            TransferManager::new(
                endpoint,
                Arc::new(CoreTransferEvents(event_bus_for_factory)),
                store_for_factory,
                file_access_for_factory,
            )
        },
    )
    .await
    .map_err(|e| CliError::NodeUnavailable(format!("启动节点失败: {e}")))?;

    // 启动清理：上次被强杀留下的 active 会话转成可恢复的 suspended。
    // **必须在节点起来之后**——转换会发事件，而事件出口这时才接好。
    // 失败不阻断启动：清理是自愈手段，它自己失败时最不该做的就是让节点起不来。
    let coordinator = swarmdrop_core::transfer::coordinator::TransferCoordinator::new(
        store.clone(),
        Arc::new(CoreTransferEvents(event_bus.clone())),
    );
    match coordinator.cleanup_recoverable_sessions().await {
        Ok(0) => {}
        Ok(n) => tracing::info!("启动清理: {n} 个中断的会话已转为可恢复"),
        Err(err) => tracing::warn!("启动清理失败（不影响启动）: {err}"),
    }

    let manager = started.manager;
    let shared = manager.shared_refs();
    swarmdrop_core::network::event_loop::spawn_event_loop(
        started.events,
        shared,
        event_bus,
        started.router,
    );

    Ok(RunningNode {
        manager,
        node_id: identity.node_id,
        secret_key: identity.secret_key,
        events: cli_events,
    })
}

/// 本机名字后面挂的宿主标识。
///
/// 括号形式而非连字符：设备列表里 `study-mac (cli)` 比 `study-mac-cli` 更容易一眼
/// 认出「这是同一台机器上的命令行」，而不是另一台叫这个名字的设备。
const CLI_SUFFIX: &str = " (cli)";

/// 探测一个能让**对端**认出这台机器的名字。
///
/// ⚠️ **不能用 `OsInfo::default().hostname`**：那一条读的是 `COMPUTERNAME` /
/// `HOSTNAME` 环境变量，而 macOS 与多数 Linux 上 `HOSTNAME` 是 shell 变量、**根本没有
/// 导出**，于是它必然落到兜底值 `Device`——同一网络里每一台 CLI 都自称 `Device (cli)`，
/// 而那个名字正是对面在决定要不要接受配对时看的第一行。
///
/// `whoami::devicename` 取的是系统里那个「电脑名称」（macOS 的共享名、Windows 的计算机名、
/// Linux 的 pretty hostname），比裸 hostname 更接近用户自己认得出的东西；拿不到再退回
/// hostname，最后才是兜底值。
fn detect_device_label() -> String {
    whoami::devicename()
        .or_else(|_| whoami::hostname())
        .unwrap_or_else(|_| "Device".to_string())
}

/// 用户没设过名字时用的那个：本机名 + `(cli)` 后缀。
///
/// **不只在首启时用**：`swarmdrop config list` 要在用户从未设过名字时给出「内置默认」
/// 那一档的值，而那必须与本函数是同一个答案——两份的话，`config list` 说的名字与节点
/// 启动后真正用的名字会不一样。
///
/// `None` 只在归一化失败时出现（本机名全是空白之类），那时退回核心自己的 hostname 回退。
pub fn default_device_name() -> Option<DeviceName> {
    // **先给后缀留出位置再截断**：`DeviceName::parse` 自己也会截到 `MAX_CHARS`，
    // 但它砍的是尾巴——那样长名字会把 ` (cli)` 整个砍掉，标识就没了。
    let budget = DeviceName::MAX_CHARS.saturating_sub(CLI_SUFFIX.chars().count());
    let label: String = detect_device_label().chars().take(budget).collect();
    DeviceName::parse(&format!("{label}{CLI_SUFFIX}"))
}

/// 首次启动时落一个能与同机图形界面宿主区分开的默认设备名。
///
/// 已有名字则原样保留——用户改过的名字不该被每次启动覆盖。
/// 失败不阻断启动：设备名只影响展示，拿不到就退回核心自己的 hostname 回退。
async fn ensure_device_name(config: &dyn DeviceConfig) {
    if config.load_device_name().await.is_some() {
        return;
    }

    let Some(name) = default_device_name() else {
        return;
    };
    if let Err(err) = config.save_device_name(Some(name)).await {
        tracing::warn!("写入默认设备名失败（不影响启动）: {err}");
    }
}
