use std::sync::Arc;

use dashmap::DashMap;
use swarm_p2p_core::NodeEvent;
use swarm_p2p_core::libp2p::{Multiaddr, PeerId};

use crate::device::{
    ConnectionType, Device, DeviceStatus, OsInfo, PairedDeviceInfo, infer_connection_type,
};
use crate::presence::PresenceMap;
use crate::protocol::AppRequest;

/// 运行时 Peer 信息（DashMap 中的值）
#[derive(Debug, Clone)]
pub(super) struct PeerInfo {
    pub peer_id: PeerId,
    pub addrs: Vec<Multiaddr>,
    pub agent_version: Option<String>,
    pub rtt_ms: Option<u64>,
    pub is_connected: bool,
    /// DCUtR 打洞是否成功（比地址推断更准确）
    pub hole_punched: bool,
    /// 发现时间戳，暂未使用但后续可用于超时清理
    #[expect(dead_code)]
    pub discovered_at: i64,
    pub connected_at: Option<i64>,
}

impl PeerInfo {
    /// 创建新发现的 Peer（未连接状态）
    fn new_discovered(peer_id: PeerId, addrs: Vec<Multiaddr>) -> Self {
        Self {
            peer_id,
            addrs,
            agent_version: None,
            rtt_ms: None,
            is_connected: false,
            hole_punched: false,
            discovered_at: chrono::Utc::now().timestamp_millis(),
            connected_at: None,
        }
    }
}

/// 设备过滤器
#[derive(Debug, Clone, serde::Deserialize, Default)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum DeviceFilter {
    #[default]
    All,
    Connected,
    Paired,
}

/// 设备管理器
///
/// 维护运行时发现的 peer，提供统一的设备查询接口。
/// 已配对设备通过共享 `Arc<DashMap>` 从 [`PairingManager`](crate::pairing::manager::PairingManager) 读取。
///
/// 本身不含 Arc，需要共享时由使用方包裹 `Arc<DeviceManager>`。
pub struct DeviceManager {
    peers: DashMap<PeerId, PeerInfo>,
    /// 与 PairingManager 共享的已配对设备（只读）
    paired_devices: Arc<DashMap<PeerId, PairedDeviceInfo>>,
    /// 与 PresenceSupervisor 共享的 presence 状态（只读）。
    /// 已配对设备的在线判定以它为准：Probing（断连宽限期）仍呈现在线。
    presence: PresenceMap,
}

impl DeviceManager {
    /// 创建 DeviceManager，传入与 PairingManager / PresenceSupervisor 共享的引用
    pub fn new(
        paired_devices: Arc<DashMap<PeerId, PairedDeviceInfo>>,
        presence: PresenceMap,
    ) -> Self {
        Self {
            peers: DashMap::new(),
            paired_devices,
            presence,
        }
    }

    /// 已配对设备的在线判定：presence 状态优先（宽限期呈现在线），
    /// Supervisor 尚未写入时回退到瞬时连接状态。
    fn paired_peer_online(&self, peer_id: &PeerId, is_connected: bool) -> bool {
        match self.presence.get(peer_id).map(|e| *e.value()) {
            Some(state) => state.is_online(),
            None => is_connected,
        }
    }

    /// 处理 NodeEvent，更新 peer 状态
    pub fn handle_event(&self, event: &NodeEvent<AppRequest>) {
        match event {
            NodeEvent::PeersDiscovered { peers } => {
                for (peer_id, addr) in peers {
                    match self.peers.get_mut(peer_id) {
                        Some(mut entry) => {
                            if !entry.addrs.contains(addr) {
                                entry.addrs.push(addr.clone());
                            }
                        }
                        None => {
                            self.peers.insert(
                                *peer_id,
                                PeerInfo::new_discovered(*peer_id, vec![addr.clone()]),
                            );
                        }
                    }
                }
            }

            NodeEvent::PeerConnected { peer_id } => {
                let now = chrono::Utc::now().timestamp_millis();
                match self.peers.get_mut(peer_id) {
                    Some(mut entry) => {
                        entry.is_connected = true;
                        entry.connected_at = Some(now);
                    }
                    None => {
                        let mut info = PeerInfo::new_discovered(*peer_id, vec![]);
                        info.is_connected = true;
                        info.connected_at = Some(now);
                        self.peers.insert(*peer_id, info);
                    }
                }
            }

            NodeEvent::PeerDisconnected { peer_id } => {
                if let Some(mut entry) = self.peers.get_mut(peer_id) {
                    entry.is_connected = false;
                    entry.rtt_ms = None;
                    entry.hole_punched = false;
                }
            }

            NodeEvent::IdentifyReceived {
                peer_id,
                agent_version,
                ..
            } => {
                if let Some(mut entry) = self.peers.get_mut(peer_id) {
                    entry.agent_version = Some(agent_version.clone());
                }
            }

            NodeEvent::PingSuccess { peer_id, rtt_ms } => {
                if let Some(mut entry) = self.peers.get_mut(peer_id) {
                    entry.rtt_ms = Some(*rtt_ms);
                }
            }

            NodeEvent::HolePunchSucceeded { peer_id } => {
                if let Some(mut entry) = self.peers.get_mut(peer_id) {
                    entry.hole_punched = true;
                }
            }

            // 其他事件忽略
            _ => {}
        }
    }

    /// 统一查询设备列表
    pub fn get_devices(&self, filter: DeviceFilter) -> Vec<Device> {
        match filter {
            DeviceFilter::All | DeviceFilter::Connected => {
                let connected_only = matches!(filter, DeviceFilter::Connected);
                self.peers
                    .iter()
                    .filter(|entry| {
                        let peer = entry.value();
                        // 只返回 SwarmDrop 客户端（过滤掉引导/中继等基础设施节点）
                        let is_app_peer = peer
                            .agent_version
                            .as_deref()
                            .is_some_and(OsInfo::is_swarmdrop_agent);
                        is_app_peer && (!connected_only || peer.is_connected)
                    })
                    .map(|entry| self.peer_to_device(entry.value()))
                    .collect()
            }
            DeviceFilter::Paired => self
                .paired_devices
                .iter()
                .map(|entry| {
                    let info = entry.value();
                    let peer_info = self.peers.get(&info.peer_id);
                    let is_connected = peer_info.as_deref().is_some_and(|p| p.is_connected);
                    let (status, connection, latency) =
                        if self.paired_peer_online(&info.peer_id, is_connected) {
                            match peer_info.as_deref() {
                                // 宽限期内连接详情沿用最近一次已知信息
                                Some(p) => connection_info(&p.addrs, p.rtt_ms, p.hole_punched),
                                None => (DeviceStatus::Online, None, None),
                            }
                        } else {
                            (DeviceStatus::Offline, None, None)
                        };

                    Device {
                        peer_id: info.peer_id,
                        os_info: info.os_info.clone(),
                        status,
                        connection,
                        latency,
                        is_paired: true,
                        trust_level: Some(info.trust_level),
                        receive_policy: Some(info.receive_policy.clone()),
                        trust_confirmed: Some(info.trust_confirmed),
                    }
                })
                .collect(),
        }
    }

    /// 将 PeerInfo 转换为 Device
    fn peer_to_device(&self, peer: &PeerInfo) -> Device {
        let os_info = peer
            .agent_version
            .as_deref()
            .and_then(OsInfo::from_agent_version)
            .unwrap_or_else(|| OsInfo::unknown_from_peer_id(&peer.peer_id));

        let paired = self.paired_devices.get(&peer.peer_id);
        // 已配对 peer 的在线判定走 presence（宽限期不闪离线）；其余 peer 用瞬时连接
        let online = if paired.is_some() {
            self.paired_peer_online(&peer.peer_id, peer.is_connected)
        } else {
            peer.is_connected
        };
        let (status, connection, latency) = if online {
            connection_info(&peer.addrs, peer.rtt_ms, peer.hole_punched)
        } else {
            (DeviceStatus::Offline, None, None)
        };
        Device {
            peer_id: peer.peer_id,
            os_info,
            status,
            connection,
            latency,
            is_paired: paired.is_some(),
            trust_level: paired.as_ref().map(|info| info.trust_level),
            receive_policy: paired.as_ref().map(|info| info.receive_policy.clone()),
            trust_confirmed: paired.as_ref().map(|info| info.trust_confirmed),
        }
    }

    /// 检查指定 peer 是否处于连接状态
    pub fn is_connected(&self, peer_id: &PeerId) -> bool {
        self.peers
            .get(peer_id)
            .is_some_and(|e| e.value().is_connected)
    }

    /// 该 peer 是否曾在局域网 mDNS 多播域内被观测到。
    ///
    /// 判据是 `addrs` 中存在私有 IP。`addrs` 的唯一写入来源是
    /// [`NodeEvent::PeersDiscovered`]（mDNS 多播实际观测到的地址），
    /// **不是**对端在 identify 里自报的 `listen_addrs`（`handle_event` 的
    /// `IdentifyReceived` 分支只取 `agent_version`）。因此远程 peer 无法
    /// 通过自报地址伪造这个判据：它压根进不了本机的多播域，`addrs` 为空。
    ///
    /// 用于给 [`PairingMethod::Direct`](crate::protocol::PairingMethod::Direct)
    /// 把关——该模式没有配对码做凭证，唯一的授权依据就是「对方确实和我在同一局域网」。
    pub fn is_lan_discovered(&self, peer_id: &PeerId) -> bool {
        self.peers
            .get(peer_id)
            .is_some_and(|e| infer_connection_type(&e.value().addrs) == Some(ConnectionType::Lan))
    }

    /// 已连接的 SwarmDrop 客户端数量
    pub fn connected_count(&self) -> usize {
        self.peers
            .iter()
            .filter(|e| {
                let p = e.value();
                p.is_connected
                    && p.agent_version
                        .as_deref()
                        .is_some_and(OsInfo::is_swarmdrop_agent)
            })
            .count()
    }

    /// 已发现的 SwarmDrop 客户端数量
    pub fn discovered_count(&self) -> usize {
        self.peers
            .iter()
            .filter(|e| {
                e.value()
                    .agent_version
                    .as_deref()
                    .is_some_and(OsInfo::is_swarmdrop_agent)
            })
            .count()
    }

    /// 是否有已连接的引导/中继节点（agent_version 以 swarm-bootstrap/ 开头）
    pub fn has_connected_bootstrap_peer(&self) -> bool {
        self.peers.iter().any(|e| {
            let p = e.value();
            p.is_connected
                && p.agent_version
                    .as_deref()
                    .is_some_and(OsInfo::is_bootstrap_agent)
        })
    }
}

/// 根据连接状态提取 (DeviceStatus, ConnectionType, latency)
///
/// `hole_punched` 为 true 时直接判定为 DCUtR，比地址推断更准确。
fn connection_info(
    addrs: &[swarm_p2p_core::libp2p::Multiaddr],
    rtt_ms: Option<u64>,
    hole_punched: bool,
) -> (DeviceStatus, Option<ConnectionType>, Option<u64>) {
    let connection = if hole_punched {
        Some(ConnectionType::Dcutr)
    } else {
        infer_connection_type(addrs)
    };
    (DeviceStatus::Online, connection, rtt_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use swarm_p2p_core::libp2p::identity::Keypair;

    fn manager() -> DeviceManager {
        DeviceManager::new(Arc::new(DashMap::new()), Arc::new(DashMap::new()))
    }

    fn peer() -> PeerId {
        PeerId::from(Keypair::generate_ed25519().public())
    }

    fn discovered(peer_id: PeerId, addr: &str) -> NodeEvent<AppRequest> {
        NodeEvent::PeersDiscovered {
            peers: vec![(peer_id, addr.parse().unwrap())],
        }
    }

    /// 远程攻击者的画像：能连上（PeerConnected），但从未出现在本机 mDNS 多播域，
    /// 因此 `addrs` 为空 —— Direct 配对必须被拒。
    #[test]
    fn peer_never_seen_on_mdns_is_not_lan_discovered() {
        let mgr = manager();
        let peer_id = peer();

        mgr.handle_event(&NodeEvent::PeerConnected { peer_id });

        assert!(mgr.is_connected(&peer_id), "前提：已连接");
        assert!(
            !mgr.is_lan_discovered(&peer_id),
            "仅连接、未经 mDNS 观测的 peer 不得被当作局域网设备"
        );
    }

    #[test]
    fn peer_discovered_on_mdns_with_private_addr_is_lan_discovered() {
        let mgr = manager();
        let peer_id = peer();

        mgr.handle_event(&discovered(peer_id, "/ip4/192.168.1.20/tcp/4001"));

        assert!(mgr.is_lan_discovered(&peer_id));
    }

    #[test]
    fn peer_with_only_public_or_relay_addr_is_not_lan_discovered() {
        let mgr = manager();
        let public_peer = peer();
        let relay_peer = peer();

        mgr.handle_event(&discovered(public_peer, "/ip4/8.8.8.8/tcp/4001"));
        mgr.handle_event(&discovered(relay_peer, "/ip4/8.8.8.8/tcp/4001/p2p-circuit"));

        assert!(
            !mgr.is_lan_discovered(&public_peer),
            "公网直连地址不是局域网"
        );
        assert!(!mgr.is_lan_discovered(&relay_peer), "中继地址不是局域网");
    }

    /// 锁死 `is_lan_discovered` 赖以成立的隐式前提：`IdentifyReceived` 里对端**自报**的
    /// `listen_addrs` 绝不能写进 `addrs`，否则远程 peer 只要谎报一个 192.168.x.x
    /// 就能把自己伪装成局域网设备，绕过 Direct 配对的唯一授权依据。
    ///
    /// 如果有人日后"顺手"让 `IdentifyReceived` 分支消费 `listen_addrs`，此测试会失败 —— 那不是
    /// 测试过时，是重新打开了一个配对绕过漏洞。
    #[test]
    fn self_reported_identify_addrs_must_not_grant_lan_status() {
        let mgr = manager();
        let peer_id = peer();

        mgr.handle_event(&NodeEvent::PeerConnected { peer_id });
        mgr.handle_event(&NodeEvent::IdentifyReceived {
            peer_id,
            agent_version: "swarmdrop/0.7.8".to_string(),
            protocol_version: "/swarmdrop/1.0.0".to_string(),
            // 攻击者自报一个私有地址，试图冒充同网段设备
            listen_addrs: vec!["/ip4/192.168.1.66/tcp/4001".parse().unwrap()],
            protocols: vec![],
        });

        assert!(
            !mgr.is_lan_discovered(&peer_id),
            "identify 自报地址不可信，不得据此授予局域网身份"
        );
    }
}
