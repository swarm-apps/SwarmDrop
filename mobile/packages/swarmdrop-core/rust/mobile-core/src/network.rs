//! 网络生命周期 —— `start_node` / `shutdown_node` / `network_status`。

//!
//! 节点开关由 host 决定（用户显式控制）；节点运行期间的 presence
//! （在线宣告 / 已配对设备保活与重连）由 core 自治，host 无需参与。

use swarmdrop_core::infra::{InfraExclusion, InfraLink, RelayLinkState};
use swarmdrop_core::network::{
    BootstrapCandidateSource, CandidateRoles, CandidateScope, CandidateSourceStatus, DiscoveryMode,
    NatStatus, NetworkRuntimeConfig, NetworkStatus as CoreNetworkStatus, NodeStatus,
};

use crate::app::MobileCore;
use crate::error::FfiResult;
use crate::events::spawn_event_loop;

#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum MobileDiscoveryMode {
    Auto,
    LanOnly,
}

impl From<DiscoveryMode> for MobileDiscoveryMode {
    fn from(mode: DiscoveryMode) -> Self {
        match mode {
            DiscoveryMode::Auto => Self::Auto,
            DiscoveryMode::LanOnly => Self::LanOnly,
        }
    }
}

impl From<MobileDiscoveryMode> for DiscoveryMode {
    fn from(mode: MobileDiscoveryMode) -> Self {
        match mode {
            MobileDiscoveryMode::Auto => Self::Auto,
            MobileDiscoveryMode::LanOnly => Self::LanOnly,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileNetworkRuntimeConfig {
    pub bootstrap_nodes: Vec<String>,
    pub discovery_mode: MobileDiscoveryMode,
    pub auto_discover_lan_helpers: bool,
    pub provide_lan_helper: bool,
    /// 公网可达性：允许经公网中继被跨网设备访问（关闭 = 严格局域网）
    pub public_reachability: bool,
}

impl From<MobileNetworkRuntimeConfig> for NetworkRuntimeConfig {
    fn from(config: MobileNetworkRuntimeConfig) -> Self {
        Self {
            bootstrap_nodes: config.bootstrap_nodes,
            discovery_mode: config.discovery_mode.into(),
            auto_discover_lan_helpers: config.auto_discover_lan_helpers,
            provide_lan_helper: config.provide_lan_helper,
            public_reachability: config.public_reachability,
        }
    }
}

/// NAT 状态的 uniffi 镜像。
///
/// 此前这一格是 `format!("{nat_status:?}")` 出来的 `"Public"`，而 JS 侧三处 UI
/// 一律判 `=== "public"`——大小写对不上，NAT 格从有这功能起就恒显示「未知」。
/// 枚举化后 JS 拿到的是判别联合，这类错配写不出来。
#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum MobileNatStatus {
    Public,
    Unknown,
}

impl From<NatStatus> for MobileNatStatus {
    fn from(status: NatStatus) -> Self {
        match status {
            NatStatus::Public => Self::Public,
            NatStatus::Unknown => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum MobileBootstrapCandidateSource {
    HostConfigured,
    MdnsLanHelper,
    Learned,
}

impl From<BootstrapCandidateSource> for MobileBootstrapCandidateSource {
    fn from(source: BootstrapCandidateSource) -> Self {
        match source {
            BootstrapCandidateSource::HostConfigured => Self::HostConfigured,
            BootstrapCandidateSource::MdnsLanHelper => Self::MdnsLanHelper,
            BootstrapCandidateSource::Learned => Self::Learned,
        }
    }
}

/// 一段基础设施关系承担的角色（两个正交能力，不是二选一）。
#[derive(Debug, Clone, Copy, uniffi::Record)]
pub struct MobileCandidateRoles {
    pub kad_server: bool,
    pub relay_server: bool,
}

impl From<CandidateRoles> for MobileCandidateRoles {
    fn from(roles: CandidateRoles) -> Self {
        // 穷尽解构 drift guard
        let CandidateRoles {
            kad_server,
            relay_server,
        } = roles;
        Self {
            kad_server,
            relay_server,
        }
    }
}

#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum MobileCandidateScope {
    Public,
    Lan,
}

impl From<CandidateScope> for MobileCandidateScope {
    fn from(scope: CandidateScope) -> Self {
        match scope {
            CandidateScope::Public => Self::Public,
            CandidateScope::Lan => Self::Lan,
        }
    }
}

/// relay reservation 三态。`lastError` 是内核原文，**不翻译**——排查时用户要贴的
/// 就是这一句。
#[derive(Debug, Clone, uniffi::Enum)]
pub enum MobileRelayLinkState {
    Connecting,
    Active { circuit_addr: String },
    Failed { last_error: String },
}

impl From<RelayLinkState> for MobileRelayLinkState {
    fn from(state: RelayLinkState) -> Self {
        match state {
            RelayLinkState::Connecting => Self::Connecting,
            RelayLinkState::Active { circuit_addr } => Self::Active {
                circuit_addr: circuit_addr.to_string(),
            },
            RelayLinkState::Failed { last_error } => Self::Failed { last_error },
        }
    }
}

/// 当前不参与 relay 收敛的原因。**说的是设置不是故障**——UI 走中性色 + 指向设置，
/// 不给「重试」。
#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum MobileInfraExclusion {
    PublicReachabilityDisabled,
}

impl From<InfraExclusion> for MobileInfraExclusion {
    fn from(reason: InfraExclusion) -> Self {
        match reason {
            InfraExclusion::PublicReachabilityDisabled => Self::PublicReachabilityDisabled,
        }
    }
}

/// 一段基础设施关系的逐条状态。
///
/// 时间跨 uniffi 一律转毫秒 `i64`（本 crate 全目录零 chrono，见 inbox / history 同例）。
#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileInfraLink {
    pub peer_id: String,
    pub addrs: Vec<String>,
    pub sources: Vec<MobileBootstrapCandidateSource>,
    pub roles: MobileCandidateRoles,
    pub scope: MobileCandidateScope,
    /// 首次登记时刻（epoch 毫秒）——宽限期的时间锚
    pub first_seen: i64,
    pub last_seen: i64,
    /// 是否给用户「移除」入口（只有来源全是 host 配置时为真）
    pub removable: bool,
    pub connected: bool,
    /// `None` = 这条关系在内核里没有 relay 轨道
    pub relay: Option<MobileRelayLinkState>,
    /// 本次会话内是否曾建立过 reservation（宽限期开关）
    pub ever_active: bool,
    pub excluded: Option<MobileInfraExclusion>,
}

impl From<InfraLink> for MobileInfraLink {
    fn from(link: InfraLink) -> Self {
        // 穷尽解构 drift guard：core 给 InfraLink 加字段时这里会编译失败。
        let InfraLink {
            peer_id,
            addrs,
            sources,
            roles,
            scope,
            first_seen,
            last_seen,
            removable,
            connected,
            relay,
            ever_active,
            excluded,
        } = link;
        Self {
            peer_id: peer_id.to_string(),
            addrs: addrs.into_iter().map(|addr| addr.to_string()).collect(),
            sources: sources.into_iter().map(Into::into).collect(),
            roles: roles.into(),
            scope: scope.into(),
            first_seen: first_seen.timestamp_millis(),
            last_seen: last_seen.timestamp_millis(),
            removable,
            connected,
            relay: relay.map(Into::into),
            ever_active,
            excluded: excluded.map(Into::into),
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileCandidateSourceStatus {
    pub source: MobileBootstrapCandidateSource,
    pub count: u64,
}

impl From<CandidateSourceStatus> for MobileCandidateSourceStatus {
    fn from(status: CandidateSourceStatus) -> Self {
        // 穷尽解构 drift guard：core 给 CandidateSourceStatus 加字段时这里会编译失败。
        let CandidateSourceStatus { source, count } = status;
        Self {
            source: source.into(),
            count: count as u64,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileNetworkStatus {
    pub status: String,
    pub peer_id: Option<String>,
    pub listen_addrs: Vec<String>,
    pub nat_status: MobileNatStatus,
    pub public_addr: Option<String>,
    pub connected_peers: u64,
    pub discovered_peers: u64,
    pub relay_ready: bool,
    /// 公网可达（活跃公网 reservation 或已确认公网直达地址）
    pub public_reachable: bool,
    /// 公网可达性设置回显（host 重启横幅检测用）
    pub public_reachability_enabled: bool,
    pub relay_peers: Vec<String>,
    pub bootstrap_connected: bool,
    pub discovery_mode: MobileDiscoveryMode,
    pub auto_discover_lan_helpers: bool,
    pub local_lan_helper_enabled: bool,
    pub local_lan_helper_running: bool,
    pub relay_server_enabled: bool,
    pub lan_helper_advertised_addrs: Vec<String>,
    pub lan_helper_count: u64,
    pub bootstrap_candidate_count: u64,
    pub candidate_sources: Vec<MobileCandidateSourceStatus>,
    pub relay_source: Option<MobileBootstrapCandidateSource>,
    /// 逐条基础设施关系。上面那批标量都是它的压扁投影，新 UI 一律读这个。
    pub infra_links: Vec<MobileInfraLink>,
}

impl From<CoreNetworkStatus> for MobileNetworkStatus {
    fn from(status: CoreNetworkStatus) -> Self {
        // 穷尽解构 drift guard：core 给 NetworkStatus 加字段时这里会编译失败。
        let CoreNetworkStatus {
            status,
            peer_id,
            listen_addrs,
            nat_status,
            public_addr,
            connected_peers,
            discovered_peers,
            relay_ready,
            public_reachable,
            public_reachability_enabled,
            relay_peers,
            bootstrap_connected,
            discovery_mode,
            auto_discover_lan_helpers,
            local_lan_helper_enabled,
            local_lan_helper_running,
            relay_server_enabled,
            lan_helper_advertised_addrs,
            lan_helper_count,
            bootstrap_candidate_count,
            candidate_sources,
            relay_source,
            infra_links,
        } = status;
        Self {
            status: match status {
                NodeStatus::Running => "running".to_string(),
                NodeStatus::Stopped => "stopped".to_string(),
            },
            peer_id: peer_id.map(|peer_id| peer_id.to_string()),
            listen_addrs: listen_addrs
                .into_iter()
                .map(|addr| addr.to_string())
                .collect(),
            nat_status: nat_status.into(),
            public_addr: public_addr.map(|addr| addr.to_string()),
            connected_peers: connected_peers as u64,
            discovered_peers: discovered_peers as u64,
            relay_ready,
            public_reachable,
            public_reachability_enabled,
            relay_peers: relay_peers
                .into_iter()
                .map(|peer_id| peer_id.to_string())
                .collect(),
            bootstrap_connected,
            discovery_mode: discovery_mode.into(),
            auto_discover_lan_helpers,
            local_lan_helper_enabled,
            local_lan_helper_running,
            relay_server_enabled,
            lan_helper_advertised_addrs: lan_helper_advertised_addrs
                .into_iter()
                .map(|addr| addr.to_string())
                .collect(),
            lan_helper_count: lan_helper_count as u64,
            bootstrap_candidate_count: bootstrap_candidate_count as u64,
            candidate_sources: candidate_sources.into_iter().map(Into::into).collect(),
            relay_source: relay_source.map(Into::into),
            infra_links: infra_links.into_iter().map(Into::into).collect(),
        }
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl MobileCore {
    pub async fn start_node(&self, network_config: MobileNetworkRuntimeConfig) -> FfiResult<()> {
        let keypair = self.ensure_keypair().await?;
        let webrtc_certificate_pem =
            swarmdrop_core::identity::load_or_create_webrtc_certificate(self.keychain()).await?;

        // 启动前先确保 SQLite 已就绪（断点续传 / 历史记录都依赖它）
        let db = self.ensure_db().await?;
        // 传输 / 收件箱端口在此**取回**而非新建：注入 TransferManager 的与 MobileCore
        // 自持的是同一个 `Arc`（启动清理也用它），不是两个包装同一条连接的实例。
        let store = self.ensure_store().await?;
        let event_bus = self.event_bus_arc() as std::sync::Arc<dyn swarmdrop_core::host::EventBus>;

        let file_access = self.file_access_arc();

        // 进程死亡时可能留下 status=Transferring 的脏会话，必须先 reconcile
        // 否则历史列表会出现"永远在传"的幽灵条目。Paused 是用户主动暂停的合法
        // 状态，不动；终态自然也不动。复用 core coordinator，转换会发 projection 事件。
        // 同时回收超期未恢复的 suspended 接收会话并清理其 .part（与桌面端对称）。
        crate::history::reconcile_stale_sessions(store.clone(), event_bus.clone(), &file_access)
            .await?;

        // 这里只供平台探测部分：移动端 env 探测拿不到 hostname，回退 "Device"，UI 走 name 字段。
        // 设备名不经 host 传入 —— core 的组合根从下面这个 device_config 端口读，本机
        // OsInfo 于是只有那一个装配点。
        let os_info = swarmdrop_core::device::OsInfo::default();

        let started = swarmdrop_core::runtime::start_node(
            keypair,
            Some(webrtc_certificate_pem),
            os_info,
            self.device_config_arc(),
            // 已配对设备的事实源是端口本身，core 内部 load 一次，host 不再预加载快照。
            self.paired_device_store_arc(),
            network_config.into(),
            swarmdrop_core::runtime::EndpointProfile::Native,
            event_bus.clone(),
            None, // 移动端无窗口聚焦概念，不需要 Notifier
            // 邀请注册表落盘，与桌面同一套 SQL 实现（openspec: invite-persistence）
            std::sync::Arc::new(swarmdrop_storage_sql::SqlInviteStore::new((*db).clone())),
            move |endpoint| {
                swarmdrop_core::transfer::manager::TransferManager::new(
                    endpoint,
                    std::sync::Arc::new(swarmdrop_core::event_adapter::CoreTransferEvents(
                        event_bus,
                    )),
                    store,
                    file_access,
                )
            },
        )
        .await?;

        // presence（宣告上线 / bootstrap / 已配对设备重连与保活）由 core 的
        // 事件循环自动接管（见 swarmdrop_core::presence），host 不再手工编排。
        let shared = started.manager.shared_refs();
        spawn_event_loop(started.events, shared, self.event_bus_arc(), started.router);

        self.set_net_manager(started.manager).await;
        Ok(())
    }

    pub async fn shutdown_node(&self) -> FfiResult<()> {
        let mut guard = self.net_manager_guard().await;
        if let Some(manager) = guard.as_ref() {
            // 宣布下线（尽力而为）+ 取消 presence 等后台任务
            manager.shutdown().await;
        }
        guard.take();
        Ok(())
    }

    pub async fn network_status(&self) -> MobileNetworkStatus {
        self.net_manager_guard()
            .await
            .as_ref()
            .map(|manager| manager.get_network_status().into())
            .unwrap_or_else(|| CoreNetworkStatus::default().into())
    }
}
