//! 设备模型和连接类型推断。

use serde::{Deserialize, Serialize};
use swarmdrop_net::{Addr, NodeId};

/// 已配对设备信任等级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum DeviceTrustLevel {
    Owned,
    #[default]
    Collaborator,
    Temporary,
    Blocked,
}

/// 自动接收时的保存行为。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum ReceiveSaveBehavior {
    /// 使用策略里配置的默认保存位置，接收完成后进入收件箱。
    #[default]
    InboxAndDefaultSaveLocation,
}

/// 可信设备接收策略。
///
/// 字段保持 host-neutral：保存位置使用字符串表达的 host 路径，桌面端解释为绝对路径，
/// 移动端后续可解释为应用文档目录下的子路径。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct DeviceReceivePolicy {
    pub auto_accept: bool,
    pub require_confirmation: bool,
    #[serde(default)]
    pub max_transfer_bytes: Option<u64>,
    pub allow_directories: bool,
    pub allow_relay_auto_accept: bool,
    #[serde(default)]
    pub save_behavior: ReceiveSaveBehavior,
    #[serde(default)]
    pub default_save_location: Option<String>,
    pub allow_mcp_send_to_device: bool,
    /// 允许 MCP/AI 代该来源设备处置入站 offer（接受或拒绝）。
    ///
    /// 默认 false。与发送侧 `allow_mcp_send_to_device` **刻意不对称**：代收会往磁盘写入、
    /// 风险更高，故即便对 Owned 设备也需用户逐设备显式开启（发送侧则随信任级别自动派生）。
    /// 只能由用户在 app 的设备信任策略中开启，agent 无任何写权限——防止自我提权、静默代收。
    #[serde(default)]
    pub allow_mcp_accept_from_device: bool,
    #[serde(default)]
    pub expires_at: Option<i64>,
}

impl Default for DeviceReceivePolicy {
    fn default() -> Self {
        Self::for_trust_level(DeviceTrustLevel::Collaborator)
    }
}

impl DeviceReceivePolicy {
    pub fn for_trust_level(level: DeviceTrustLevel) -> Self {
        match level {
            DeviceTrustLevel::Owned => Self {
                auto_accept: true,
                require_confirmation: false,
                max_transfer_bytes: None,
                allow_directories: true,
                allow_relay_auto_accept: true,
                save_behavior: ReceiveSaveBehavior::InboxAndDefaultSaveLocation,
                default_save_location: None,
                allow_mcp_send_to_device: true,
                allow_mcp_accept_from_device: false,
                expires_at: None,
            },
            DeviceTrustLevel::Collaborator => Self {
                auto_accept: false,
                require_confirmation: true,
                max_transfer_bytes: None,
                allow_directories: true,
                allow_relay_auto_accept: false,
                save_behavior: ReceiveSaveBehavior::InboxAndDefaultSaveLocation,
                default_save_location: None,
                allow_mcp_send_to_device: false,
                allow_mcp_accept_from_device: false,
                expires_at: None,
            },
            DeviceTrustLevel::Temporary => Self {
                auto_accept: false,
                require_confirmation: true,
                max_transfer_bytes: Some(512 * 1024 * 1024),
                allow_directories: false,
                allow_relay_auto_accept: false,
                save_behavior: ReceiveSaveBehavior::InboxAndDefaultSaveLocation,
                default_save_location: None,
                allow_mcp_send_to_device: false,
                allow_mcp_accept_from_device: false,
                expires_at: Some(chrono::Utc::now().timestamp_millis() + 24 * 60 * 60 * 1000),
            },
            DeviceTrustLevel::Blocked => Self {
                auto_accept: false,
                require_confirmation: false,
                max_transfer_bytes: Some(0),
                allow_directories: false,
                allow_relay_auto_accept: false,
                save_behavior: ReceiveSaveBehavior::InboxAndDefaultSaveLocation,
                default_save_location: None,
                allow_mcp_send_to_device: false,
                allow_mcp_accept_from_device: false,
                expires_at: None,
            },
        }
    }
}

/// 设备操作系统信息。
///
/// `hostname` 是系统主机名（运行时取，桌面端通常是机器名，移动端通常拿不到）；
/// `name` 是用户在 onboarding / 设置里起的名字（持久化，host 注入），UI 显示按
/// `name.as_deref().unwrap_or(&hostname)` 回退。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct OsInfo {
    /// 用户起的设备名；缺省时回退到 `hostname`。
    #[serde(default)]
    pub name: Option<String>,
    pub hostname: String,
    pub os: String,
    pub platform: String,
    pub arch: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
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
            capabilities: Vec::new(),
        }
    }
}

impl OsInfo {
    /// 不含任何设备信息的占位值，仅用于保持 wire 格式兼容。
    ///
    /// [`OnlineRecord`](crate::presence::OnlineRecord) 的 `os_info` 是历史遗留的**死字段**：
    /// 它随在线宣告发布到公共 DHT（key = `SHA256(NS‖peer_id)`，任何人可算可查、记录无签名），
    /// 但读取端只取 `dialable_addrs()`，从不消费它。而 [`OsInfo::default`] 会带上
    /// `COMPUTERNAME`/`HOSTNAME` —— 等于每 150 秒向一个公开 keyspace 广播一次主机名，
    /// 而主机名常含真名。
    ///
    /// **为什么不直接删掉字段**：`hostname`/`os`/`platform`/`arch` 都没有 `#[serde(default)]`，
    /// 删掉会让存量客户端反序列化整条记录失败（进而丢掉 `direct_addrs`，退化成盲拨）。
    /// 发空值则 wire 格式不变、存量客户端零影响。
    ///
    /// 随 presence 重写（改为「只对已配对设备可见」）时，整个 `os_info` 字段应一并移除。
    pub fn redacted() -> Self {
        Self {
            name: None,
            hostname: String::new(),
            os: String::new(),
            platform: String::new(),
            arch: String::new(),
            capabilities: Vec::new(),
        }
    }

    /// SwarmDrop 客户端 agent_version 前缀。
    pub const AGENT_PREFIX: &str = "swarmdrop/";

    /// 引导/中继节点 agent_version 前缀（swarm-bootstrap）。
    pub const BOOTSTRAP_AGENT_PREFIX: &str = "swarm-bootstrap/";

    /// 局域网协助节点 capability。
    pub const LAN_HELPER_CAPABILITY: &str = "lan-helper";

    /// 检查 agent_version 是否属于 SwarmDrop 客户端。
    pub fn is_swarmdrop_agent(agent_version: &str) -> bool {
        agent_version.starts_with(Self::AGENT_PREFIX)
    }

    /// 检查 agent_version 是否属于引导/中继节点。
    pub fn is_bootstrap_agent(agent_version: &str) -> bool {
        agent_version.starts_with(Self::BOOTSTRAP_AGENT_PREFIX)
    }

    pub fn has_capability(&self, capability: &str) -> bool {
        self.capabilities.iter().any(|cap| cap == capability)
    }

    pub fn with_capability(mut self, capability: impl Into<String>) -> Self {
        let capability = capability.into();
        if !self.capabilities.contains(&capability) {
            self.capabilities.push(capability);
        }
        self
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
        let caps_part = if self.capabilities.is_empty() {
            String::new()
        } else {
            format!("; caps={}", self.capabilities.join(","))
        };
        format!(
            "swarmdrop/{}{}{}; os={}; platform={}; arch={}; host={}",
            env!("CARGO_PKG_VERSION"),
            name_part,
            caps_part,
            self.os,
            self.platform,
            self.arch,
            self.hostname
        )
    }

    /// 无法解析 agent_version 时的回退值，用 NodeId 末尾 8 位作为 hostname。
    pub fn unknown_from_peer_id(peer_id: &NodeId) -> Self {
        let s = peer_id.to_string();
        Self {
            name: None,
            hostname: s[s.len().saturating_sub(8)..].to_string(),
            os: "unknown".to_string(),
            platform: "unknown".to_string(),
            arch: "unknown".to_string(),
            capabilities: Vec::new(),
        }
    }

    /// 从 agent_version 字符串反解析出 OsInfo。
    pub fn from_agent_version(agent_version: &str) -> Option<Self> {
        let mut name = None;
        let mut os = None;
        let mut platform = None;
        let mut arch = None;
        let mut hostname = None;
        let mut capabilities = Vec::new();

        for part in agent_version.split("; ") {
            if let Some(v) = part.strip_prefix("name=") {
                name = Some(v.to_string());
            } else if let Some(v) = part.strip_prefix("caps=") {
                capabilities.extend(
                    v.split(',')
                        .map(str::trim)
                        .filter(|cap| !cap.is_empty())
                        .map(str::to_string),
                );
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
            capabilities,
        })
    }
}

/// 已配对设备信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PairedDeviceInfo {
    #[cfg_attr(feature = "specta", specta(type = String))]
    pub peer_id: NodeId,
    #[serde(flatten)]
    pub os_info: OsInfo,
    pub paired_at: i64,
    #[serde(default)]
    pub trust_level: DeviceTrustLevel,
    #[serde(default)]
    pub receive_policy: DeviceReceivePolicy,
    #[serde(default)]
    pub trust_confirmed: bool,
}

impl PairedDeviceInfo {
    pub fn new(peer_id: NodeId, os_info: OsInfo, paired_at: i64) -> Self {
        let trust_level = DeviceTrustLevel::Collaborator;
        Self {
            peer_id,
            os_info,
            paired_at,
            trust_level,
            receive_policy: DeviceReceivePolicy::for_trust_level(trust_level),
            trust_confirmed: true,
        }
    }

    pub fn apply_trust_level_defaults(&mut self, trust_level: DeviceTrustLevel) {
        self.trust_level = trust_level;
        self.receive_policy = DeviceReceivePolicy::for_trust_level(trust_level);
        self.trust_confirmed = true;
    }

    /// 用对端 Identify 广播的最新信息刷新设备元数据。
    ///
    /// 返回是否发生了变化，供调用方避免重复持久化。
    pub fn refresh_os_info(&mut self, os_info: OsInfo) -> bool {
        if self.os_info == os_info {
            return false;
        }
        self.os_info = os_info;
        true
    }
}

/// 设备状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    pub peer_id: NodeId,
    #[serde(flatten)]
    pub os_info: OsInfo,
    pub status: DeviceStatus,
    pub connection: Option<ConnectionType>,
    pub latency: Option<u64>,
    pub is_paired: bool,
    pub trust_level: Option<DeviceTrustLevel>,
    pub receive_policy: Option<DeviceReceivePolicy>,
    pub trust_confirmed: Option<bool>,
}

/// 设备列表查询结果。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct DeviceListResult {
    pub devices: Vec<Device>,
    pub total: usize,
}

/// 基于地址分析推断连接类型。
///
/// 分类谓词收口于 [`Addr`]（迁自旧栈散落三处的手写位运算）：私网/loopback→局域网，
/// 公网可路由→打洞直连，circuit→中继。优先级 LAN > DCUtR > Relay。
pub fn infer_connection_type(addrs: &[Addr]) -> Option<ConnectionType> {
    if addrs.is_empty() {
        return None;
    }

    let mut has_lan = false;
    let mut has_dcutr = false;
    let mut has_relay = false;

    for addr in addrs {
        if addr.is_circuit() {
            has_relay = true;
        } else if addr.is_private_lan() || addr.is_loopback() {
            has_lan = true;
        } else if addr.is_public_routable() {
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

#[cfg(test)]
mod tests {
    use swarmdrop_net::SecretKey;

    use super::{DeviceTrustLevel, OsInfo, PairedDeviceInfo};

    fn sample(name: Option<&str>, hostname: &str) -> OsInfo {
        OsInfo {
            name: name.map(str::to_string),
            hostname: hostname.to_string(),
            os: "macos".to_string(),
            platform: "macos".to_string(),
            arch: "aarch64".to_string(),
            capabilities: Vec::new(),
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
    fn agent_version_roundtrip_with_capability() {
        let info = sample(Some("桌面端"), "Desktop").with_capability(OsInfo::LAN_HELPER_CAPABILITY);
        let agent = info.to_agent_version();
        assert!(agent.contains("caps=lan-helper"), "got: {agent}");
        let parsed = OsInfo::from_agent_version(&agent).unwrap();
        assert!(parsed.has_capability(OsInfo::LAN_HELPER_CAPABILITY));
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
        assert!(info.capabilities.is_empty());
    }

    #[test]
    fn deserialize_legacy_paired_device_requires_trust_confirmation() {
        let peer_id = SecretKey::generate().node_id();
        let json = serde_json::json!({
            "peerId": peer_id.to_string(),
            "hostname": "old-phone",
            "os": "ios",
            "platform": "ios",
            "arch": "aarch64",
            "pairedAt": 42
        });

        let device: PairedDeviceInfo = serde_json::from_value(json).unwrap();

        assert_eq!(device.trust_level, DeviceTrustLevel::Collaborator);
        assert!(device.receive_policy.require_confirmation);
        assert!(!device.receive_policy.auto_accept);
        assert!(!device.trust_confirmed);
    }

    #[test]
    fn refresh_os_info_updates_remote_device_name() {
        let peer_id = SecretKey::generate().node_id();
        let mut device = PairedDeviceInfo::new(peer_id, sample(None, "MacBook-Pro"), 42);

        let changed = device.refresh_os_info(sample(Some("小李的 MacBook"), "MacBook-Pro"));

        assert!(changed);
        assert_eq!(device.os_info.name.as_deref(), Some("小李的 MacBook"));
    }

    #[test]
    fn allow_mcp_accept_defaults_false_all_trust_levels() {
        // 代收是显式授权：任何信任级别（含本人设备）默认都不开。
        for level in [
            DeviceTrustLevel::Owned,
            DeviceTrustLevel::Collaborator,
            DeviceTrustLevel::Temporary,
            DeviceTrustLevel::Blocked,
        ] {
            let policy = super::DeviceReceivePolicy::for_trust_level(level);
            assert!(
                !policy.allow_mcp_accept_from_device,
                "代收默认应关闭: {level:?}"
            );
        }
    }

    #[test]
    fn legacy_policy_without_allow_mcp_accept_deserializes_false() {
        // 旧持久化的 receive_policy 没有 allowMcpAcceptFromDevice 字段，应回落 false。
        let json = serde_json::json!({
            "autoAccept": true,
            "requireConfirmation": false,
            "allowDirectories": true,
            "allowRelayAutoAccept": true,
            "allowMcpSendToDevice": true,
        });
        let policy: super::DeviceReceivePolicy = serde_json::from_value(json).unwrap();
        assert!(!policy.allow_mcp_accept_from_device);
    }
}
