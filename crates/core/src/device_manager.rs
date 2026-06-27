use std::sync::Arc;

use dashmap::DashMap;
use swarm_p2p_core::NodeEvent;
use swarm_p2p_core::libp2p::{Multiaddr, PeerId};

use crate::device::{
    ConnectionType, Device, DeviceStatus, OsInfo, PairedDeviceInfo, infer_connection_type,
};
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
}

impl DeviceManager {
    /// 创建 DeviceManager，传入与 PairingManager 共享的已配对设备引用
    pub fn new(paired_devices: Arc<DashMap<PeerId, PairedDeviceInfo>>) -> Self {
        Self {
            peers: DashMap::new(),
            paired_devices,
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
                    let (status, connection, latency) = match peer_info.as_deref() {
                        Some(p) if p.is_connected => {
                            connection_info(&p.addrs, p.rtt_ms, p.hole_punched)
                        }
                        _ => (DeviceStatus::Offline, None, None),
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

        let (status, connection, latency) = if peer.is_connected {
            connection_info(&peer.addrs, peer.rtt_ms, peer.hole_punched)
        } else {
            (DeviceStatus::Offline, None, None)
        };

        let paired = self.paired_devices.get(&peer.peer_id);
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
