//! 设备模型和连接类型推断。

use serde::{Deserialize, Serialize};
use swarm_p2p_core::libp2p::{multiaddr::Protocol, Multiaddr, PeerId};

/// 设备操作系统信息。
///
/// `hostname` 是系统主机名（运行时取，桌面端通常是机器名，移动端通常拿不到）；
/// `name` 是用户在 onboarding / 设置里起的名字（持久化，host 注入），UI 显示按
/// `name.as_deref().unwrap_or(&hostname)` 回退。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct OsInfo {
    /// 用户起的设备名；缺省时回退到 `hostname`。
    #[serde(default)]
    pub name: Option<String>,
    pub hostname: String,
    pub os: String,
    pub platform: String,
    pub arch: String,
}

impl Default for OsInfo {
    fn default() -> Self {
        // 移动端拿不到这两个环境变量，会落到 "Device" —— 此时 UI 走 name 字段。
        let hostname = std::env::var("COMPUTERNAME")
            .or_else(|_| std::env::var("HOSTNAME"))
            .unwrap_or_else(|_| "Device".to_string());

        Self {
            name: None,
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

    /// Encode as `agent_version`.
    ///
    /// 带 name：`swarmdrop/{ver}; name={name}; os=...; platform=...; arch=...; host=...`
    /// 不带：  `swarmdrop/{ver}; os=...; platform=...; arch=...; host=...`
    ///
    /// 仅当 `name` 与 `hostname` 不同时才写入 `name=` 槽位，避免冗余。
    pub fn to_agent_version(&self) -> String {
        let name_part = self
            .name
            .as_deref()
            .filter(|n| *n != self.hostname)
            .map(|n| format!("; name={n}"))
            .unwrap_or_default();
        format!(
            "swarmdrop/{}{}; os={}; platform={}; arch={}; host={}",
            env!("CARGO_PKG_VERSION"),
            name_part,
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
            name: None,
            hostname: s[s.len().saturating_sub(8)..].to_string(),
            os: "unknown".to_string(),
            platform: "unknown".to_string(),
            arch: "unknown".to_string(),
        }
    }

    /// 从 agent_version 字符串反解析出 OsInfo。
    pub fn from_agent_version(agent_version: &str) -> Option<Self> {
        let mut name = None;
        let mut os = None;
        let mut platform = None;
        let mut arch = None;
        let mut hostname = None;

        for part in agent_version.split("; ") {
            if let Some(v) = part.strip_prefix("name=") {
                name = Some(v.to_string());
            } else if let Some(v) = part.strip_prefix("os=") {
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
            name,
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

#[cfg(test)]
mod tests {
    use super::OsInfo;

    fn sample(name: Option<&str>, hostname: &str) -> OsInfo {
        OsInfo {
            name: name.map(str::to_string),
            hostname: hostname.to_string(),
            os: "macos".to_string(),
            platform: "macos".to_string(),
            arch: "aarch64".to_string(),
        }
    }

    #[test]
    fn agent_version_roundtrip_without_name() {
        let info = sample(None, "MacBook-Pro");
        let agent = info.to_agent_version();
        assert!(!agent.contains("name="), "no name= when unset: {agent}");
        let parsed = OsInfo::from_agent_version(&agent).unwrap();
        assert_eq!(parsed.name, None);
        assert_eq!(parsed.hostname, "MacBook-Pro");
    }

    #[test]
    fn agent_version_roundtrip_with_name() {
        let info = sample(Some("光印的 iPhone"), "Device");
        let agent = info.to_agent_version();
        assert!(agent.contains("name=光印的 iPhone"), "got: {agent}");
        let parsed = OsInfo::from_agent_version(&agent).unwrap();
        assert_eq!(parsed.name.as_deref(), Some("光印的 iPhone"));
        assert_eq!(parsed.hostname, "Device");
    }

    #[test]
    fn agent_version_skips_name_when_equals_hostname() {
        // name == hostname 时不写 name= 槽位，避免冗余
        let info = sample(Some("MacBook-Pro"), "MacBook-Pro");
        let agent = info.to_agent_version();
        assert!(!agent.contains("name="), "got: {agent}");
    }

    #[test]
    fn default_no_name() {
        assert_eq!(OsInfo::default().name, None);
    }

    #[test]
    fn deserialize_legacy_payload_without_name_field() {
        // 旧 paired_devices.json 没有 name 字段，应反序列化成 None
        let json = r#"{"hostname":"old","os":"macos","platform":"macos","arch":"aarch64"}"#;
        let info: OsInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.name, None);
        assert_eq!(info.hostname, "old");
    }
}
