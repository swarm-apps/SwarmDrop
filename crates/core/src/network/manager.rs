use std::sync::{Arc, RwLock};

use dashmap::DashMap;
use swarmdrop_net::{Addr, Endpoint, NodeAddr, NodeId, RelayState};
use tokio_util::sync::CancellationToken;

use super::candidates::{BootstrapCandidateSource, CandidateRoles, CandidateScope};
use super::config::NetworkRuntimeConfig;
use super::{BootstrapCandidateManager, NetworkStatus, NodeStatus};
use crate::device::PairedDeviceInfo;
use crate::device_manager::DeviceManager;
use crate::error::{AppError, AppResult};
use crate::host::{EventBus, Notifier};
use crate::infra::InfraSupervisor;
use crate::pairing::manager::PairingManager;
use crate::presence::{PresenceMap, PresenceSupervisor};

// TransferRuntime 端口随 transfer 域迁出（消费方 NetManager 在 core，实现方
// TransferManager 在 transfer，端口定义在下层 transfer 以免 transfer 反依赖 core）。
// re-export 保持 `crate::network::TransferRuntime` 路径不变。
pub use crate::transfer::runtime::TransferRuntime;

/// 网络管理器
///
/// 统一管理 [`Endpoint`]、[`DeviceManager`] 和 [`PairingManager`]，对
/// [`commands`](crate::commands) 层提供访问接口。运行时网络状态不再镜像到本地
/// `Arc<RwLock>` 字段，而是直接读 `endpoint.watch_*()`（last-value-wins 采样）。
pub struct NetManager<TTransfer = ()> {
    endpoint: Endpoint,
    pairing: Arc<PairingManager>,
    devices: Arc<DeviceManager>,
    presence: Arc<PresenceSupervisor>,
    infra: Arc<InfraSupervisor>,
    transfer: Arc<TTransfer>,
    /// 全局取消令牌（shutdown 时取消所有后台任务）
    cancel_token: CancellationToken,
    candidates: Arc<RwLock<BootstrapCandidateManager>>,
    network_config: NetworkRuntimeConfig,
}

impl<TTransfer> NetManager<TTransfer>
where
    TTransfer: TransferRuntime,
{
    pub fn new(
        endpoint: Endpoint,
        paired_devices: Vec<PairedDeviceInfo>,
        transfer: TTransfer,
        network_config: NetworkRuntimeConfig,
        candidates: BootstrapCandidateManager,
        event_bus: Arc<dyn EventBus>,
        notifier: Option<Arc<dyn Notifier>>,
    ) -> Self {
        // 创建共享的已配对设备 Map：PairingManager 读写，DeviceManager 只读
        let paired_map: Arc<DashMap<_, _>> = Arc::new(
            paired_devices
                .into_iter()
                .map(|info| (info.peer_id, info))
                .collect(),
        );

        let candidates = Arc::new(RwLock::new(candidates));
        // presence 状态表：Supervisor 写，DeviceManager 读（在线判定）
        let presence_map: PresenceMap = Arc::new(DashMap::new());
        let presence = Arc::new(PresenceSupervisor::new(
            endpoint.clone(),
            paired_map.clone(),
            presence_map.clone(),
            candidates.clone(),
        ));
        let devices = Arc::new(DeviceManager::new(paired_map.clone(), presence_map));
        // pairing 需要 devices（Direct 的局域网校验）+ event_bus + notifier
        let pairing = Arc::new(PairingManager::new(
            endpoint.clone(),
            paired_map,
            devices.clone(),
            event_bus,
            notifier,
        ));
        // 基础设施链路收敛：候选表为期望状态源，reservation 断线自动重建
        let infra = Arc::new(InfraSupervisor::new(
            endpoint.clone(),
            candidates.clone(),
            network_config.public_reachability,
        ));
        let transfer = Arc::new(transfer);
        let cancel_token = CancellationToken::new();

        // 启动传输资源超时清理任务
        TTransfer::spawn_cleanup_task(&transfer, cancel_token.clone());

        Self {
            endpoint,
            pairing,
            devices,
            presence,
            infra,
            transfer,
            cancel_token,
            candidates,
            network_config,
        }
    }

    pub fn pairing(&self) -> &PairingManager {
        &self.pairing
    }

    pub fn pairing_arc(&self) -> Arc<PairingManager> {
        self.pairing.clone()
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

    /// 手动登记一个 relay helper 的常驻可达意图（幂等，重复登记只合并地址）。
    ///
    /// 只写期望状态——候选表 `UserCustom` + relay 角色；真正的拨号 /
    /// reservation / 断线重建全部由 [`InfraSupervisor`] 统一收敛（单一收敛
    /// 路径，最迟下一个 1s tick 启动第一轮）。进度经
    /// `endpoint().watch_relays()` 观测（Connecting / Active / Failed）。
    pub fn ensure_relay_intent(&self, helper: NodeAddr) {
        let scope = CandidateScope::infer(&helper.addrs);
        if let Ok(mut candidates) = self.candidates.write() {
            candidates.upsert(
                helper.id,
                helper.addrs,
                BootstrapCandidateSource::UserCustom,
                CandidateRoles {
                    kad_server: false,
                    relay_server: true,
                },
                scope,
            );
        }
    }

    /// 撤销 relay 常驻意图（[`ensure_relay_intent`](Self::ensure_relay_intent)
    /// 的对称面）：清候选表与收敛状态，并注销内核侧登记——关 circuit
    /// listener、立刻断开（含中止在途拨号），此后不存在任何重建路径。
    pub async fn remove_relay_intent(&self, node: NodeId) -> AppResult<()> {
        if let Ok(mut candidates) = self.candidates.write() {
            candidates.remove(node);
        }
        self.infra.remove(node);
        self.endpoint
            .remove_infrastructure_peer(node)
            .await
            .map_err(|e| AppError::Network(e.to_string()))
    }

    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    /// 取消所有后台任务（shutdown 时调用）
    pub fn cancel_background_tasks(&self) {
        self.cancel_token.cancel();
    }

    /// 停止节点前的收尾：宣布下线（尽力而为）+ 取消全部后台任务 + 关闭内核。
    ///
    /// 关闭 Endpoint 会 drop 底层 Swarm（断开全部连接、关闭监听），对端据此判离线；
    /// Router 的 accept 循环随入站流源关闭而退出，无需单独编排。
    pub async fn shutdown(&self) {
        if let Err(e) = self.presence.announce_offline().await {
            tracing::debug!("announce_offline 失败（忽略）: {e}");
        }
        self.cancel_background_tasks();
        self.endpoint.close().await;
    }

    /// 获取当前网络状态快照
    pub fn get_network_status(&self) -> NetworkStatus {
        self.shared_refs().build_network_status()
    }

    /// 获取事件循环需要的共享引用
    pub fn shared_refs(&self) -> SharedNetRefs<TTransfer> {
        SharedNetRefs {
            endpoint: self.endpoint.clone(),
            devices: self.devices.clone(),
            pairing: self.pairing.clone(),
            presence: self.presence.clone(),
            infra: self.infra.clone(),
            cancel_token: self.cancel_token.clone(),
            transfer: self.transfer.clone(),
            candidates: self.candidates.clone(),
            network_config: self.network_config.clone(),
        }
    }
}

/// 事件循环使用的共享引用
///
/// 持有与 [`NetManager`] 相同的 Arc 引用，供
/// [`run_event_loop`](super::event_loop::run_event_loop) 在独立 tokio task 中
/// 处理内核事件与网络状态。
pub struct SharedNetRefs<TTransfer = ()> {
    pub endpoint: Endpoint,
    pub devices: Arc<DeviceManager>,
    pub pairing: Arc<PairingManager>,
    pub presence: Arc<PresenceSupervisor>,
    pub infra: Arc<InfraSupervisor>,
    /// 全局取消令牌（presence/infra 等后台任务随之退出）
    pub cancel_token: CancellationToken,
    pub transfer: Arc<TTransfer>,
    pub candidates: Arc<RwLock<BootstrapCandidateManager>>,
    pub network_config: NetworkRuntimeConfig,
}

// 手写 Clone：全部字段为 Arc/Clone，避免给 TTransfer 加 Clone 约束
impl<TTransfer> Clone for SharedNetRefs<TTransfer> {
    fn clone(&self) -> Self {
        Self {
            endpoint: self.endpoint.clone(),
            devices: self.devices.clone(),
            pairing: self.pairing.clone(),
            presence: self.presence.clone(),
            infra: self.infra.clone(),
            cancel_token: self.cancel_token.clone(),
            transfer: self.transfer.clone(),
            candidates: self.candidates.clone(),
            network_config: self.network_config.clone(),
        }
    }
}

impl<TTransfer> SharedNetRefs<TTransfer> {
    /// 当前持有活跃 reservation 的中继节点列表（本机经它们被动可达）。
    pub fn active_relay_peers(&self) -> Vec<NodeId> {
        // with()：只读 key 无需深拷贝整个 map（RelayState 三态化后 value 含堆数据）
        self.endpoint.watch_relays().with(|map| {
            map.iter()
                .filter(|(_, state)| matches!(state, RelayState::Active { .. }))
                .map(|(peer, _)| *peer)
                .collect()
        })
    }

    /// 构建当前网络状态快照
    pub fn build_network_status(&self) -> NetworkStatus {
        let relay_peers_list = self.active_relay_peers();
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

        // watch_addrs 只读一次（原先 listen_addrs()/public_addr() 各读一遍各深拷贝）
        let addrs = self.endpoint.watch_addrs().get();
        let public_addr = addrs.external.first().cloned();
        let listen_addrs = addrs.listen;
        // 公网可达 = AutoNAT 确认的公网直达地址，或任一公网范围 relay 的活跃 reservation
        let public_reachable = public_addr.is_some()
            || relay_peers_list.iter().any(|peer| {
                candidate_snapshot
                    .as_deref()
                    .and_then(|c| c.get(*peer))
                    .is_some_and(|c| matches!(c.scope, CandidateScope::Public))
            });

        // LanHelper：本机若配置为提供协助，则把私网监听地址作为可公告地址。
        let lan_helper_advertised_addrs: Vec<Addr> = if self.network_config.provide_lan_helper {
            listen_addrs
                .iter()
                .filter(|a| a.is_private_lan())
                .cloned()
                .collect()
        } else {
            Vec::new()
        };

        NetworkStatus {
            status: NodeStatus::Running,
            peer_id: Some(self.endpoint.node_id()),
            listen_addrs,
            nat_status: self.endpoint.watch_nat().get(),
            public_addr,
            connected_peers: self.devices.connected_count(),
            discovered_peers: self.devices.discovered_count(),
            relay_ready: !relay_peers_list.is_empty(),
            public_reachable,
            public_reachability_enabled: self.network_config.public_reachability,
            relay_peers: relay_peers_list,
            bootstrap_connected: self.devices.has_connected_bootstrap_peer(),
            discovery_mode: self.network_config.discovery_mode,
            auto_discover_lan_helpers: self.network_config.auto_discover_lan_helpers,
            // 迁移后三者同源于 `provide_lan_helper` 配置位——新内核 relay server
            // 装配在 bind 期、无运行时开关，`local_lan_helper_running` 不再是运行时事实
            // （只要配置开就恒为 true）。语义待前端协同收敛（UI 重写任务）。
            local_lan_helper_enabled: self.network_config.provide_lan_helper,
            local_lan_helper_running: self.network_config.provide_lan_helper,
            relay_server_enabled: self.network_config.provide_lan_helper,
            lan_helper_advertised_addrs,
            lan_helper_count,
            bootstrap_candidate_count,
            candidate_sources,
            relay_source,
        }
    }
}
