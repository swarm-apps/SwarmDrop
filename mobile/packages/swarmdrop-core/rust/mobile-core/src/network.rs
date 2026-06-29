//! 网络生命周期 —— `start_node` / `shutdown_node` / `network_status`。
//!
//! host 必须在前台主动调 `start_node`,后台时调 `shutdown_node`。core 不自动管。

use swarmdrop_core::network::{
    BootstrapCandidateSource, CandidateSourceStatus, DiscoveryMode, NetworkRuntimeConfig,
    NetworkStatus as CoreNetworkStatus, NodeStatus,
};

use crate::app::MobileCore;
use crate::error::{FfiError, FfiResult};
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
    pub custom_bootstrap_nodes: Vec<String>,
    pub discovery_mode: MobileDiscoveryMode,
    pub auto_discover_lan_helpers: bool,
    pub provide_lan_helper: bool,
}

impl From<MobileNetworkRuntimeConfig> for NetworkRuntimeConfig {
    fn from(config: MobileNetworkRuntimeConfig) -> Self {
        Self {
            custom_bootstrap_nodes: config.custom_bootstrap_nodes,
            discovery_mode: config.discovery_mode.into(),
            auto_discover_lan_helpers: config.auto_discover_lan_helpers,
            provide_lan_helper: config.provide_lan_helper,
        }
    }
}

#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum MobileBootstrapCandidateSource {
    BuiltInPublic,
    UserCustom,
    MdnsLanHelper,
}

impl From<BootstrapCandidateSource> for MobileBootstrapCandidateSource {
    fn from(source: BootstrapCandidateSource) -> Self {
        match source {
            BootstrapCandidateSource::BuiltInPublic => Self::BuiltInPublic,
            BootstrapCandidateSource::UserCustom => Self::UserCustom,
            BootstrapCandidateSource::MdnsLanHelper => Self::MdnsLanHelper,
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
        Self {
            source: status.source.into(),
            count: status.count as u64,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileNetworkStatus {
    pub status: String,
    pub peer_id: Option<String>,
    pub listen_addrs: Vec<String>,
    pub nat_status: String,
    pub public_addr: Option<String>,
    pub connected_peers: u64,
    pub discovered_peers: u64,
    pub relay_ready: bool,
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
}

impl From<CoreNetworkStatus> for MobileNetworkStatus {
    fn from(status: CoreNetworkStatus) -> Self {
        Self {
            status: match status.status {
                NodeStatus::Running => "running".to_string(),
                NodeStatus::Stopped => "stopped".to_string(),
            },
            peer_id: status.peer_id.map(|peer_id| peer_id.to_string()),
            listen_addrs: status
                .listen_addrs
                .into_iter()
                .map(|addr| addr.to_string())
                .collect(),
            nat_status: format!("{:?}", status.nat_status),
            public_addr: status.public_addr.map(|addr| addr.to_string()),
            connected_peers: status.connected_peers as u64,
            discovered_peers: status.discovered_peers as u64,
            relay_ready: status.relay_ready,
            relay_peers: status
                .relay_peers
                .into_iter()
                .map(|peer_id| peer_id.to_string())
                .collect(),
            bootstrap_connected: status.bootstrap_connected,
            discovery_mode: status.discovery_mode.into(),
            auto_discover_lan_helpers: status.auto_discover_lan_helpers,
            local_lan_helper_enabled: status.local_lan_helper_enabled,
            local_lan_helper_running: status.local_lan_helper_running,
            relay_server_enabled: status.relay_server_enabled,
            lan_helper_advertised_addrs: status
                .lan_helper_advertised_addrs
                .into_iter()
                .map(|addr| addr.to_string())
                .collect(),
            lan_helper_count: status.lan_helper_count as u64,
            bootstrap_candidate_count: status.bootstrap_candidate_count as u64,
            candidate_sources: status
                .candidate_sources
                .into_iter()
                .map(Into::into)
                .collect(),
            relay_source: status.relay_source.map(Into::into),
        }
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl MobileCore {
    pub async fn start_node(
        &self,
        device_name: Option<String>,
        network_config: MobileNetworkRuntimeConfig,
    ) -> FfiResult<()> {
        let keypair = self.ensure_keypair().await?;
        let paired_devices = swarmdrop_core::identity::load_paired_devices(self.keychain()).await?;

        // 启动前先确保 SQLite 已就绪（断点续传 / 历史记录都依赖它）
        let db = self.ensure_db().await?;
        let event_bus =
            self.event_bus_arc() as std::sync::Arc<dyn swarmdrop_core::host::EventBus>;

        // 进程死亡时可能留下 status=Transferring 的脏会话，必须先 reconcile
        // 否则历史列表会出现"永远在传"的幽灵条目。Paused 是用户主动暂停的合法
        // 状态，不动；终态自然也不动。复用 core coordinator，转换会发 projection 事件。
        crate::history::reconcile_stale_sessions(db.clone(), event_bus.clone()).await?;

        let file_access = self.file_access_arc();

        let started = swarmdrop_core::runtime::start_node(
            keypair,
            device_name,
            paired_devices,
            network_config.into(),
            move |client, data_channel_rx| {
                swarmdrop_core::transfer::manager::TransferManager::new(
                    client,
                    event_bus,
                    db,
                    file_access,
                    data_channel_rx,
                )
            },
        )?;

        let shared = started.manager.shared_refs();
        let client = started.manager.client().clone();
        let pairing = shared.pairing.clone();
        tokio::spawn(async move {
            let _ = pairing.announce_online().await;
            let _ = client.bootstrap().await;
            pairing.check_paired_online().await;
        });
        spawn_event_loop(started.receiver, shared, self.event_bus_arc());

        self.set_net_manager(started.manager).await;
        Ok(())
    }

    pub async fn shutdown_node(&self) -> FfiResult<()> {
        let mut guard = self.net_manager_guard().await;
        if let Some(manager) = guard.as_ref() {
            manager
                .pairing()
                .announce_offline()
                .await
                .map_err(FfiError::from)?;
            manager.cancel_background_tasks();
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
