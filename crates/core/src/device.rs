//! 设备模型和连接类型推断。

use serde::{Deserialize, Serialize};
use swarm_p2p_core::libp2p::{multiaddr::Protocol, Multiaddr, PeerId};

/// 设备操作系统信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct OsInfo {
    pub hostname: String,
    pub os: String,
    pub platform: String,
    pub arch: String,
}

impl Default for OsInfo {
    fn default() -> Self {
        let hostname = std::env::var("COMPUTERNAME")
            .or_else(|_| std::env::var("HOSTNAME"))
            .unwrap_or_else(|_| "unknown".to_string());

        Self {
            hostname,
            os: std::env::consts::OS.to_string(),
            platform: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
        }
    }
}

impl OsInfo {
    /// SwarmDrop 客户端 agent_version 前缀。
    pub const AGENT_PREFIX: &str = "swarmdrop/";

    /// 引导/中继节点 agent_version 前缀（swarm-bootstrap）。
    pub const BOOTSTRAP_AGENT_PREFIX: &str = "swarm-bootstrap/";

    /// 检查 agent_version 是否属于 SwarmDrop 客户端。
    pub fn is_swarmdrop_agent(agent_version: &str) -> bool {
        agent_version.starts_with(Self::AGENT_PREFIX)
    }

    /// 检查 agent_version 是否属于引导/中继节点。
    pub fn is_bootstrap_agent(agent_version: &str) -> bool {
        agent_version.starts_with(Self::BOOTSTRAP_AGENT_PREFIX)
    }

    pub fn to_agent_version(&self) -> String {
        format!(
            "swarmdrop/{}; os={}; platform={}; arch={}; host={}",
            env!("CARGO_PKG_VERSION"),
            self.os,
            self.platform,
            self.arch,
            self.hostname
        )
    }

    /// 无法解析 agent_version 时的回退值，用 PeerId 末尾 8 位作为 hostname。
    pub fn unknown_from_peer_id(peer_id: &PeerId) -> Self {
        let s = peer_id.to_string();
        Self {
            hostname: s[s.len().saturating_sub(8)..].to_string(),
            os: "unknown".to_string(),
            platform: "unknown".to_string(),
            arch: "unknown".to_string(),
        }
    }

    /// 从 agent_version 字符串反解析出 OsInfo。
    pub fn from_agent_version(agent_version: &str) -> Option<Self> {
        let mut os = None;
        let mut platform = None;
        let mut arch = None;
        let mut hostname = None;

        for part in agent_version.split("; ") {
            if let Some(v) = part.strip_prefix("os=") {
                os = Some(v.to_string());
            } else if let Some(v) = part.strip_prefix("platform=") {
                platform = Some(v.to_string());
            } else if let Some(v) = part.strip_prefix("arch=") {
                arch = Some(v.to_string());
            } else if let Some(v) = part.strip_prefix("host=") {
                hostname = Some(v.to_string());
            }
        }

        Some(Self {
            hostname: hostname?,
            os: os?,
            platform: platform?,
            arch: arch?,
        })
    }
}

/// 已配对设备信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PairedDeviceInfo {
    #[cfg_attr(feature = "specta", specta(type = String))]
    pub peer_id: PeerId,
    #[serde(flatten)]
    pub os_info: OsInfo,
    pub paired_at: i64,
}

/// 设备状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum DeviceStatus {
    Online,
    Offline,
}

/// 连接类型。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum ConnectionType {
    Lan,
    Dcutr,
    Relay,
}

/// 统一的设备输出类型。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Device {
    #[cfg_attr(feature = "specta", specta(type = String))]
    pub peer_id: PeerId,
    #[serde(flatten)]
    pub os_info: OsInfo,
    pub status: DeviceStatus,
    pub connection: Option<ConnectionType>,
    pub latency: Option<u64>,
    pub is_paired: bool,
}

/// 设备列表查询结果。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct DeviceListResult {
    pub devices: Vec<Device>,
    pub total: usize,
}

/// 基于 Multiaddr 分析推断连接类型。
pub fn infer_connection_type(addrs: &[Multiaddr]) -> Option<ConnectionType> {
    if addrs.is_empty() {
        return None;
    }

    let mut has_lan = false;
    let mut has_dcutr = false;
    let mut has_relay = false;

    for addr in addrs {
        if has_p2p_circuit(addr) {
            has_relay = true;
        } else if has_private_ip(addr) {
            has_lan = true;
        } else if has_public_ip(addr) {
            has_dcutr = true;
        }
    }

    if has_lan {
        Some(ConnectionType::Lan)
    } else if has_dcutr {
        Some(ConnectionType::Dcutr)
    } else if has_relay {
        Some(ConnectionType::Relay)
    } else {
        None
    }
}

fn has_p2p_circuit(addr: &Multiaddr) -> bool {
    addr.iter().any(|p| matches!(p, Protocol::P2pCircuit))
}

fn has_private_ip(addr: &Multiaddr) -> bool {
    addr.iter().any(|p| {
        matches!(p, Protocol::Ip4(ip) if ip.is_private() || ip.is_loopback() || ip.is_link_local())
    })
}

fn has_public_ip(addr: &Multiaddr) -> bool {
    addr.iter().any(|p| {
        matches!(p, Protocol::Ip4(ip) if !ip.is_private() && !ip.is_loopback() && !ip.is_link_local() && !ip.is_unspecified())
    })
}
