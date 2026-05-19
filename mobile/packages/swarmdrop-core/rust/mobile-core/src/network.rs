//! 网络生命周期 —— `start_node` / `shutdown_node` / `network_status`。
//!
//! host 必须在前台主动调 `start_node`,后台时调 `shutdown_node`。core 不自动管。

use swarmdrop_core::network::{NetworkStatus as CoreNetworkStatus, NodeStatus};

use crate::app::MobileCore;
use crate::error::{FfiError, FfiResult};
use crate::events::spawn_event_loop;

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
        }
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl MobileCore {
    pub async fn start_node(
        &self,
        device_name: Option<String>,
        custom_bootstrap_nodes: Vec<String>,
    ) -> FfiResult<()> {
        let keypair = self.ensure_keypair().await?;
        let paired_devices =
            swarmdrop_core::identity::load_paired_devices(self.keychain()).await?;

        // 启动前先确保 SQLite 已就绪（断点续传 / 历史记录都依赖它）
        let db = self.ensure_db().await?;
        let event_bus = self.event_bus_arc()
            as std::sync::Arc<dyn swarmdrop_core::host::EventBus>;
        let file_access = self.file_access_arc();

        let started = swarmdrop_core::runtime::start_node(
            keypair,
            device_name,
            paired_devices,
            custom_bootstrap_nodes,
            move |client| {
                swarmdrop_core::transfer::manager::TransferManager::new(
                    client,
                    event_bus,
                    db,
                    file_access,
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
