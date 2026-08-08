use std::sync::{Arc, RwLock};

use dashmap::DashMap;
use swarmdrop_net::{Addr, Endpoint, NodeAddr, NodeId, RelayState};
use tokio_util::sync::CancellationToken;

use super::candidates::{BootstrapCandidateSource, CandidateRoles};
use super::config::NetworkRuntimeConfig;
use super::{BootstrapCandidateManager, NetworkStatus, NodeStatus};
use crate::device::{DeviceName, OsInfo, PairedDeviceInfo};
use crate::device_manager::DeviceManager;
use crate::error::{AppError, AppResult};
use crate::host::{EventBus, Notifier, PairedDeviceStore};
use crate::infra::InfraSupervisor;
use crate::pairing::manager::PairingManager;
use crate::presence::{PresenceMap, PresenceSupervisor};
use swarmdrop_invite::InviteStore;

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
    /// `invite_store` 是邀请注册表的落盘端口（native = `SqlInviteStore` /
    /// wasm = IndexedDB 实现），由宿主注入；不需要持久化时传
    /// `Arc::new(NoopInviteStore)`。构造后宿主应 `await pairing().load_invites()`。
    ///
    /// `paired_devices` 与 `paired_store` **不是冗余**：构造是同步的，拿不到 `.await`，
    /// 所以由调用方（`runtime::start_node`）先从同一个端口 load 出快照建共享 `DashMap`，
    /// 端口本身再转交 `PairingManager` 供 [`unpair`](PairingManager::unpair) 写回。
    /// 两者同源，不是两个事实源。
    ///
    /// `os_info` 是**本机**设备信息（平台探测 + 用户设备名），由 `runtime::start_node`
    /// 装配后透传给 [`PairingManager`]——配对请求与邀请串的 display 都读它。
    #[expect(
        clippy::too_many_arguments,
        reason = "与 runtime::start_node 同源：参数都是三端各自供给的端口/身份/配置，\
                  打包成 struct 只是换个容器、不减调用方负担"
    )]
    pub fn new(
        endpoint: Endpoint,
        os_info: OsInfo,
        paired_devices: Vec<PairedDeviceInfo>,
        transfer: TTransfer,
        network_config: NetworkRuntimeConfig,
        candidates: BootstrapCandidateManager,
        event_bus: Arc<dyn EventBus>,
        notifier: Option<Arc<dyn Notifier>>,
        invite_store: Arc<dyn InviteStore>,
        paired_store: Arc<dyn PairedDeviceStore>,
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
            os_info,
            paired_map,
            devices.clone(),
            event_bus,
            notifier,
            invite_store,
            paired_store,
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

    /// 改本机设备名的**两步一致性入口**：内存态 + identify 的 `agent_version`，
    /// 一次做完，返回更新后的完整 [`OsInfo`]。
    ///
    /// 这两步必须成对，此前只写在文档里：`PairingManager::set_device_name` 只改内存态
    /// （新发的邀请、出站配对请求的 display 读它），identify 那半要调用方另起一步。
    /// 谁单独调一次前者，就得到「新邀请写着新名字、已连接的对端 identify 到的还是旧名字」
    /// ——一个没有任何报错、只能靠肉眼在两台设备之间对比才发现的偏差。收进一个方法之后
    /// `set_device_name` 降为 `pub(crate)`，crate 外拿不到「只做一半」的那个动作。
    ///
    /// `name = None` 表示清空，回退系统 hostname。归一化由 [`DeviceName::parse`]
    /// 在更外层完成——本方法只认已归一化的值。
    ///
    /// 落盘**不在这里**：它属于 [`DeviceConfig`](crate::host::DeviceConfig) 端口，且必须
    /// 排在推网络之前（先广播再落盘一旦落盘失败，用户看到的是「改成功了」、重启却变回
    /// 旧名字）。完整四步顺序见 [`rename_device`](crate::device_name::rename_device)。
    pub async fn set_local_device_name(&self, name: Option<DeviceName>) -> AppResult<OsInfo> {
        // ① 内存态：本机 OsInfo 的唯一写口，返回完整快照供 ② 重算 agent_version
        //    （agent_version 里还有 caps= 槽位，不能由调用方自己拼串）。
        let os_info = self.pairing.set_device_name(name);
        // ② identify：既有连接原地更新 + 向已连接对端主动 push，一个 RTT 内生效。
        //    未连接的对端不排队补推——它们下次连上时 handler 直接取新值。
        self.endpoint
            .set_agent_version(os_info.to_agent_version())
            .await
            .map_err(|e| AppError::Network(format!("广播新设备名失败: {e}")))?;
        Ok(os_info)
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

    /// 手动登记一个基础设施节点的常驻意图（幂等，重复登记只合并地址与角色）。
    ///
    /// 只写期望状态——候选表 `HostConfigured` + 给定角色；真正的拨号 /
    /// reservation / 断线重建全部由 [`InfraSupervisor`] 统一收敛（单一收敛
    /// 路径，最迟下一个 1s tick 启动第一轮）。进度经
    /// `endpoint().watch_relays()` 观测（Connecting / Active / Failed）。
    ///
    /// **角色要传全（[`CandidateRoles::kad_and_relay`]）。** 它此前写死
    /// `{ kad_server: false, relay_server: true }`，于是浏览器端手动登记的节点
    /// 进不了 kad 路由表——今天没出事只因为 `learn_candidate` 会在 identify 之后
    /// 把 kad 角色补回来，那是巧合不是设计。
    pub fn ensure_infra_intent(&self, peer: NodeAddr, roles: CandidateRoles) {
        if let Ok(mut candidates) = self.candidates.write() {
            candidates.upsert(
                peer.id,
                peer.addrs,
                BootstrapCandidateSource::HostConfigured,
                roles,
            );
        }
    }

    /// 撤销基础设施常驻意图（[`ensure_infra_intent`](Self::ensure_infra_intent)
    /// 的对称面）：清候选表与收敛状态，并注销内核侧登记——关 circuit
    /// listener、立刻断开（含中止在途拨号）。
    ///
    /// 此处的直接注销调用是**低延迟快路径**（用户操作立即生效，不等 tick）；
    /// 「不复活」的最终保证来自 supervisor 的反向收敛环——即便本调用与在途
    /// 注册任务竞态、登记被短暂复活，环在下一轮差集必然清理，二者幂等叠加。
    pub async fn remove_infra_intent(&self, node: NodeId) -> AppResult<()> {
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

    /// 构建当前网络状态快照。
    ///
    /// `infra_links` 是逐条基础设施关系的完整状态；下面那批标量是它的压扁投影，
    /// 现在**从它派生**（事实源收敛到一处），字段与语义对外不变——`relay_ready`
    /// 是 MCP agent 面 schema，`lan_helper_count` / `candidate_sources` 是 e2e
    /// 断言载体，删不得。
    ///
    /// 两个例外保持原实现：`bootstrap_connected` 与 `discovered_peers` 的口径是
    /// 「扫全部已连 peer 的 agent 前缀」，与候选表是**不同集合**（候选表有学习上限
    /// 与可用地址两道闸），改成派生会静默翻转这一位。
    pub fn build_network_status(&self) -> NetworkStatus {
        let infra_links = crate::infra::build_infra_links(self);
        // **`relay_peers` 必须从同一份 `infra_links` 派生，不能再读一次 `watch_relays`。**
        // 那是两次独立采样：中间掉一个 reservation，就会发出
        // `relay_ready: true` + `infra_links[p].relay: Failed` 这种自相矛盾的状态，
        // 而 `infra_reconcile.rs` 的断言正好禁止它——两次读会让那条测试变成偶发红。
        let relay_peers_list: Vec<NodeId> = infra_links
            .iter()
            .filter(|l| matches!(l.relay, Some(crate::infra::RelayLinkState::Active { .. })))
            .map(|l| l.peer_id)
            .collect();
        let candidate_sources = self
            .candidates
            .read()
            .ok()
            .as_deref()
            .map(BootstrapCandidateManager::source_statuses)
            .unwrap_or_default();
        let lan_helper_count = infra_links
            .iter()
            .filter(|l| {
                l.sources
                    .contains(&crate::network::BootstrapCandidateSource::MdnsLanHelper)
            })
            .count();
        let bootstrap_candidate_count = infra_links.len();
        let relay_source = relay_peers_list.first().and_then(|peer_id| {
            infra_links
                .iter()
                .find(|l| l.peer_id == *peer_id)
                .and_then(|l| l.sources.first().copied())
        });

        // watch_addrs 只读一次（原先 listen_addrs()/public_addr() 各读一遍各深拷贝）
        let addrs = self.endpoint.watch_addrs().get();
        let public_addr = addrs.external.first().cloned();
        let listen_addrs = addrs.listen;
        // 公网可达 = AutoNAT 确认的公网直达地址，或任一**持有公网地址**的 relay 的活跃 reservation。
        //
        // **判据是地址而不是 `scope`。** `scope` 是收敛环的**放行闸**（`infer` 的语义是
        // 「任一私网地址即判 Lan」，为的是不让公网开关拦下用户手点的本地 helper），
        // 而 `upsert` 现在按累加后的全部地址重算它、地址表又只增不减——于是一台公网中继
        // 只要被 mDNS 顺带看到过一次私网地址，`scope` 就永久停在 `Lan` 再也回不去，
        // 本机会在整个会话里谎报「不可公网可达」。放行闸与能力声明是两件事，不能共用一个位。
        //
        // **保持析取、不压成单枚举**：直达与中继在领域里并存且分别下发
        // （presence 的 classify_announce_addrs 把地址分成两组一起进 OnlineRecord），
        // 压成互斥四选一就再也说不出「直连地址有了、但中继全挂了」这句退避期最常见的话。
        let public_reachable = public_addr.is_some()
            || infra_links.iter().any(|l| {
                matches!(l.relay, Some(crate::infra::RelayLinkState::Active { .. }))
                    && l.addrs
                        .iter()
                        .any(|a| !a.is_private_lan() && !a.is_loopback())
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
            infra_links,
        }
    }
}
