use std::collections::HashSet;
use std::sync::{Arc, RwLock};

use dashmap::DashMap;
use swarm_p2p_core::libp2p::{Multiaddr, PeerId};
use tokio_util::sync::CancellationToken;

use super::candidates::CandidateScope;
use super::config::NetworkRuntimeConfig;
use super::{BootstrapCandidateManager, NatStatus, NetworkStatus, NodeStatus};
use crate::device::PairedDeviceInfo;
use crate::device_manager::DeviceManager;
use crate::infra::InfraSupervisor;
use crate::pairing::manager::PairingManager;
use crate::presence::{PresenceMap, PresenceSupervisor};
use crate::protocol::AppNetClient;

/// NetManager 注入的传输运行时。
pub trait TransferRuntime: Send + Sync + 'static {
    fn spawn_cleanup_task(transfer: &Arc<Self>, cancel_token: CancellationToken);
}

impl TransferRuntime for () {
    fn spawn_cleanup_task(_transfer: &Arc<Self>, _cancel_token: CancellationToken) {}
}

/// 网络管理器
///
/// 统一管理 [`AppNetClient`]、[`DeviceManager`] 和 [`PairingManager`]，
/// 对 [`commands`](crate::commands) 层提供访问接口。
pub struct NetManager<TTransfer = ()> {
    client: AppNetClient,
    peer_id: PeerId,
    pairing: Arc<PairingManager>,
    devices: Arc<DeviceManager>,
    presence: Arc<PresenceSupervisor>,
    infra: Arc<InfraSupervisor>,
    transfer: Arc<TTransfer>,
    /// 全局取消令牌（shutdown 时取消所有后台任务）
    cancel_token: CancellationToken,
    // 网络状态（Arc<RwLock> 供事件循环并发更新）
    listen_addrs: Arc<RwLock<Vec<Multiaddr>>>,
    nat_status: Arc<RwLock<NatStatus>>,
    public_addr: Arc<RwLock<Option<Multiaddr>>>,
    /// 当前已连接的中继节点 PeerId 集合
    relay_peers: Arc<RwLock<HashSet<PeerId>>>,
    candidates: Arc<RwLock<BootstrapCandidateManager>>,
    network_config: NetworkRuntimeConfig,
    lan_helper_advertised_addrs: Arc<RwLock<Vec<Multiaddr>>>,
    relay_server_enabled: Arc<RwLock<bool>>,
}

impl<TTransfer> NetManager<TTransfer>
where
    TTransfer: TransferRuntime,
{
    pub fn new(
        client: AppNetClient,
        peer_id: PeerId,
        paired_devices: Vec<PairedDeviceInfo>,
        transfer: TTransfer,
        network_config: NetworkRuntimeConfig,
        candidates: BootstrapCandidateManager,
    ) -> Self {
        // 创建共享的已配对设备 Map：PairingManager 读写，DeviceManager 只读
        let paired_map: Arc<DashMap<_, _>> = Arc::new(
            paired_devices
                .into_iter()
                .map(|info| (info.peer_id, info))
                .collect(),
        );

        let pairing = Arc::new(PairingManager::new(
            client.clone(),
            peer_id,
            paired_map.clone(),
        ));
        let candidates = Arc::new(RwLock::new(candidates));
        // presence 状态表：Supervisor 写，DeviceManager 读（在线判定）
        let presence_map: PresenceMap = Arc::new(DashMap::new());
        let presence = Arc::new(PresenceSupervisor::new(
            client.clone(),
            peer_id,
            paired_map.clone(),
            presence_map.clone(),
            candidates.clone(),
        ));
        let devices = Arc::new(DeviceManager::new(paired_map, presence_map));
        // 基础设施链路收敛：候选表为期望状态源，reservation 断线自动重建
        let infra = Arc::new(InfraSupervisor::new(
            client.clone(),
            candidates.clone(),
            network_config.public_reachability,
        ));
        let transfer = Arc::new(transfer);
        let cancel_token = CancellationToken::new();

        // 启动传输资源超时清理任务
        TTransfer::spawn_cleanup_task(&transfer, cancel_token.clone());

        Self {
            client,
            peer_id,
            pairing,
            devices,
            presence,
            infra,
            transfer,
            cancel_token,
            listen_addrs: Arc::new(RwLock::new(Vec::new())),
            nat_status: Arc::new(RwLock::new(NatStatus::Unknown)),
            public_addr: Arc::new(RwLock::new(None)),
            relay_peers: Arc::new(RwLock::new(HashSet::new())),
            candidates,
            network_config,
            lan_helper_advertised_addrs: Arc::new(RwLock::new(Vec::new())),
            relay_server_enabled: Arc::new(RwLock::new(false)),
        }
    }

    pub fn pairing(&self) -> &PairingManager {
        &self.pairing
    }

    pub fn devices(&self) -> &DeviceManager {
        &self.devices
    }

    pub fn transfer(&self) -> &TTransfer {
        &self.transfer
    }

    pub fn transfer_arc(&self) -> Arc<TTransfer> {
        self.transfer.clone()
    }

    pub fn client(&self) -> &AppNetClient {
        &self.client
    }

    /// 取消所有后台任务（shutdown 时调用）
    pub fn cancel_background_tasks(&self) {
        self.cancel_token.cancel();
    }

    /// 停止节点前的收尾：宣布下线（尽力而为）+ 取消全部后台任务。
    ///
    /// host 停止节点只需调用本方法，无需关心 presence 细节。
    pub async fn shutdown(&self) {
        if let Err(e) = self.presence.announce_offline().await {
            tracing::debug!("announce_offline 失败（忽略）: {e}");
        }
        self.cancel_background_tasks();
    }

    /// 获取当前网络状态快照
    pub fn get_network_status(&self) -> NetworkStatus {
        self.shared_refs().build_network_status()
    }

    /// 获取事件循环需要的共享引用
    pub fn shared_refs(&self) -> SharedNetRefs<TTransfer> {
        SharedNetRefs {
            peer_id: self.peer_id,
            client: self.client.clone(),
            devices: self.devices.clone(),
            pairing: self.pairing.clone(),
            presence: self.presence.clone(),
            infra: self.infra.clone(),
            cancel_token: self.cancel_token.clone(),
            transfer: self.transfer.clone(),
            listen_addrs: self.listen_addrs.clone(),
            nat_status: self.nat_status.clone(),
            public_addr: self.public_addr.clone(),
            relay_peers: self.relay_peers.clone(),
            candidates: self.candidates.clone(),
            network_config: self.network_config.clone(),
            lan_helper_advertised_addrs: self.lan_helper_advertised_addrs.clone(),
            relay_server_enabled: self.relay_server_enabled.clone(),
        }
    }
}

/// 事件循环使用的共享引用
///
/// 持有与 [`NetManager`] 相同的 Arc 引用，
/// 供 [`spawn_event_loop`](super::spawn_event_loop) 在独立 tokio task 中更新网络状态。
pub struct SharedNetRefs<TTransfer = ()> {
    pub peer_id: PeerId,
    pub client: AppNetClient,
    pub devices: Arc<DeviceManager>,
    pub pairing: Arc<PairingManager>,
    pub presence: Arc<PresenceSupervisor>,
    pub infra: Arc<InfraSupervisor>,
    /// 全局取消令牌（presence/infra 等后台任务随之退出）
    pub cancel_token: CancellationToken,
    pub transfer: Arc<TTransfer>,
    pub listen_addrs: Arc<RwLock<Vec<Multiaddr>>>,
    pub nat_status: Arc<RwLock<NatStatus>>,
    pub public_addr: Arc<RwLock<Option<Multiaddr>>>,
    pub relay_peers: Arc<RwLock<HashSet<PeerId>>>,
    pub candidates: Arc<RwLock<BootstrapCandidateManager>>,
    pub network_config: NetworkRuntimeConfig,
    pub lan_helper_advertised_addrs: Arc<RwLock<Vec<Multiaddr>>>,
    pub relay_server_enabled: Arc<RwLock<bool>>,
}

// 手写 Clone：全部字段为 Arc/Clone，避免给 TTransfer 加 Clone 约束
impl<TTransfer> Clone for SharedNetRefs<TTransfer> {
    fn clone(&self) -> Self {
        Self {
            peer_id: self.peer_id,
            client: self.client.clone(),
            devices: self.devices.clone(),
            pairing: self.pairing.clone(),
            presence: self.presence.clone(),
            infra: self.infra.clone(),
            cancel_token: self.cancel_token.clone(),
            transfer: self.transfer.clone(),
            listen_addrs: self.listen_addrs.clone(),
            nat_status: self.nat_status.clone(),
            public_addr: self.public_addr.clone(),
            relay_peers: self.relay_peers.clone(),
            candidates: self.candidates.clone(),
            network_config: self.network_config.clone(),
            lan_helper_advertised_addrs: self.lan_helper_advertised_addrs.clone(),
            relay_server_enabled: self.relay_server_enabled.clone(),
        }
    }
}

impl<TTransfer> SharedNetRefs<TTransfer> {
    /// 构建当前网络状态快照
    pub fn build_network_status(&self) -> NetworkStatus {
        let relay_peers_list: Vec<PeerId> = self
            .relay_peers
            .read()
            .map(|g| g.iter().copied().collect())
            .unwrap_or_default();
        let candidate_snapshot = self.candidates.read().ok();
        let candidate_sources = candidate_snapshot
            .as_deref()
            .map(BootstrapCandidateManager::source_statuses)
            .unwrap_or_default();
        let lan_helper_count = candidate_snapshot
            .as_deref()
            .map(BootstrapCandidateManager::lan_helper_count)
            .unwrap_or_default();
        let bootstrap_candidate_count = candidate_snapshot
            .as_deref()
            .map(BootstrapCandidateManager::candidate_count)
            .unwrap_or_default();
        let relay_source = relay_peers_list
            .first()
            .and_then(|peer_id| candidate_snapshot.as_deref()?.relay_source(*peer_id));
        let public_addr = self.public_addr.read().ok().and_then(|g| g.clone());
        // 公网可达 = AutoNAT 确认的公网直达地址，或任一公网范围 relay 的活跃 reservation
        let public_reachable = public_addr.is_some()
            || relay_peers_list.iter().any(|peer| {
                candidate_snapshot
                    .as_deref()
                    .and_then(|c| c.get(*peer))
                    .is_some_and(|c| matches!(c.scope, CandidateScope::Public))
            });

        NetworkStatus {
            status: NodeStatus::Running,
            peer_id: Some(self.peer_id),
            listen_addrs: read_or(&self.listen_addrs, Vec::new()),
            nat_status: read_or(&self.nat_status, NatStatus::Unknown),
            public_addr,
            connected_peers: self.devices.connected_count(),
            discovered_peers: self.devices.discovered_count(),
            relay_ready: !relay_peers_list.is_empty(),
            public_reachable,
            relay_peers: relay_peers_list,
            bootstrap_connected: self.devices.has_connected_bootstrap_peer(),
            discovery_mode: self.network_config.discovery_mode,
            auto_discover_lan_helpers: self.network_config.auto_discover_lan_helpers,
            local_lan_helper_enabled: self.network_config.provide_lan_helper,
            local_lan_helper_running: self.network_config.provide_lan_helper
                && *self
                    .relay_server_enabled
                    .read()
                    .as_deref()
                    .unwrap_or(&false),
            relay_server_enabled: *self
                .relay_server_enabled
                .read()
                .as_deref()
                .unwrap_or(&false),
            lan_helper_advertised_addrs: read_or(&self.lan_helper_advertised_addrs, Vec::new()),
            lan_helper_count,
            bootstrap_candidate_count,
            candidate_sources,
            relay_source,
        }
    }
}

/// 读取 RwLock，中毒时返回默认值
fn read_or<T: Clone>(lock: &RwLock<T>, default: T) -> T {
    lock.read().map(|g| g.clone()).unwrap_or(default)
}
