//! 设备模型和连接类型推断。

use serde::{Deserialize, Serialize};
use swarmdrop_net_base::{Addr, NodeId};

// 链路详情的组成部分之一，随 `ConnectionDetails` 一起进 IPC/FFI——
// 消费方（uniffi 桥接、wasm 壳）从这里取，不必再依赖 net-base。
pub use swarmdrop_net_base::TransportKind;

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
        Self::for_trust_level(DeviceTrustLevel::Collaborator, None)
    }
}

impl DeviceReceivePolicy {
    /// 某信任级别的接收策略。**三端唯一的事实源**——桌面、移动、Web 都经各自的 binding
    /// 调这里，不许再抄一份到 JS（那正是本函数收 `previous` 之前的状态：两份 JS 副本各自
    /// 长出了不同的「保留哪些字段」规则，而内核这一份一个都不保留）。
    ///
    /// `previous` 是该设备**当前**的策略；新配对传 `None`。切换信任级别时，两项
    /// **用户显式设过的东西**要带过去，不能被默认值抹掉：
    ///
    /// - `default_save_location` —— 用户选的自动接收落点。丢了它 `auto_accept` 就是一张
    ///   空头支票：[`evaluate_receive_policy`](crate::device) 的消费方在这一项为空时一律
    ///   退回手动确认（「未配置自动接收保存位置」）。也就是说「升到本人设备」这个动作会
    ///   悄悄关掉自动接收——用户看到的开关还开着。
    /// - `allow_mcp_accept_from_device` —— 代 AI 收件的授权。它只能由用户显式开启
    ///   （见字段注释），那么级别变化既不该替他重新授权，也不该替他撤销。
    ///
    /// **`Blocked` 是唯一例外**：两项都清零。「已阻止」必须是一个不留后门的终态，
    /// 而不是「阻止了但保存位置和代收授权还留着」。
    pub fn for_trust_level(level: DeviceTrustLevel, previous: Option<&Self>) -> Self {
        // 非 blocked 分支统一从这里取；blocked 分支不看它们。
        let default_save_location = previous.and_then(|p| p.default_save_location.clone());
        let allow_mcp_accept_from_device = previous.is_some_and(|p| p.allow_mcp_accept_from_device);

        match level {
            DeviceTrustLevel::Owned => Self {
                auto_accept: true,
                require_confirmation: false,
                max_transfer_bytes: None,
                allow_directories: true,
                allow_relay_auto_accept: true,
                save_behavior: ReceiveSaveBehavior::InboxAndDefaultSaveLocation,
                default_save_location,
                allow_mcp_send_to_device: true,
                allow_mcp_accept_from_device,
                expires_at: None,
            },
            DeviceTrustLevel::Collaborator => Self {
                auto_accept: false,
                require_confirmation: true,
                max_transfer_bytes: None,
                allow_directories: true,
                allow_relay_auto_accept: false,
                save_behavior: ReceiveSaveBehavior::InboxAndDefaultSaveLocation,
                default_save_location,
                allow_mcp_send_to_device: false,
                allow_mcp_accept_from_device,
                expires_at: None,
            },
            DeviceTrustLevel::Temporary => Self {
                auto_accept: false,
                require_confirmation: true,
                max_transfer_bytes: Some(512 * 1024 * 1024),
                allow_directories: false,
                allow_relay_auto_accept: false,
                save_behavior: ReceiveSaveBehavior::InboxAndDefaultSaveLocation,
                default_save_location,
                allow_mcp_send_to_device: false,
                allow_mcp_accept_from_device,
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

/// 用户设置的设备名——**已归一化**的不可变值。
///
/// 唯一构造入口是 [`DeviceName::parse`]，未经归一化的 `String` 在类型层面就进不了
/// [`DeviceConfig`](crate::ports::DeviceConfig) 端口。之所以做成 newtype 而不是一个
/// 「各调用点自觉调用」的归一化自由函数：设备名的入口有桌面 IPC 命令、移动 uniffi 导出、
/// wasm 导出三处，将来只会更多，漏掉任何一处都等于归一化不存在。
///
/// 归一化顺序：
/// 1. `trim()`；
/// 2. 剥掉控制字符与 `;`；
/// 3. 截断到 [`MAX_CHARS`](Self::MAX_CHARS) 个 **char**（不是 byte——中文名 40 字要占
///    120 字节，按 byte 截断还会切碎多字节序列）；
/// 4. 再 `trim()` 一次（第 2、3 步可能在首尾留下空白，不补这一下 `parse` 就不是幂等的），
///    结果为空则返回 `None`——「空」即清空，语义上等于回退到 [`OsInfo::hostname`]。
///
/// **为什么必须剥 `;`**：[`OsInfo::to_agent_version`] 用 `"; "` 拼字段、
/// [`OsInfo::from_agent_version`] 按 `"; "` 切片再按 `name=` / `caps=` 前缀分派。设备名里
/// 带一个 `"; caps=lan-helper"`，对端就会解析出本机并不具备的 capability，进而把本机当成
/// 局域网协助节点（kad server + relay 候选）。归一化在此处一次性关掉这条注入。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DeviceName(String);

impl DeviceName {
    /// 设备名长度上限（char 数）。三端 UI 的 `maxLength` 与之对齐，后端截断只是防御纵深。
    pub const MAX_CHARS: usize = 40;

    /// 归一化并构造设备名；结果为空返回 `None`（= 清空，回退 hostname）。
    pub fn parse(raw: &str) -> Option<Self> {
        let cleaned: String = raw
            .trim()
            .chars()
            .filter(|c| !c.is_control() && *c != ';')
            .take(Self::MAX_CHARS)
            .collect();
        let cleaned = cleaned.trim();
        (!cleaned.is_empty()).then(|| Self(cleaned.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

/// 设备操作系统信息。
///
/// `hostname` 是系统主机名（运行时取，桌面端通常是机器名，移动端通常拿不到）；
/// `name` 是用户在 onboarding / 设置里起的名字（持久化），由 core 的组合根从
/// [`DeviceConfig`](crate::ports::DeviceConfig) 端口填充，UI 显示按
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
    /// Native 端（桌面 / 移动）的装配入口：运行时探测 hostname / os / arch，`name` 留空。
    /// Web 端另有 `web_os_info()`，不走这里。
    ///
    /// `name` 由 core 的组合根（`start_node`）从
    /// [`DeviceConfig`](crate::ports::DeviceConfig) 端口填充——宿主**没有** API 可以注入它。
    /// 这不是审美：设备名一旦能从宿主侧塞进来，三端就会各读各的持久化、各自漏掉
    /// [`DeviceName`] 的归一化，而本机 `OsInfo` 也就不再有唯一装配点。
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

    /// UI 显示名：`name` 去空白后非空则用它，否则回退 `hostname`。
    ///
    /// 收敛「name → hostname」回退语义于一处，避免各端（transfer / mobile / pairing / web）
    /// 各手写一份、对「空串是否回退 / 是否 trim」处理分叉。
    pub fn display_name(&self) -> String {
        self.name
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .unwrap_or(&self.hostname)
            .to_string()
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
            // 新配对，没有「上一份策略」可带。
            receive_policy: DeviceReceivePolicy::for_trust_level(trust_level, None),
            trust_confirmed: true,
        }
    }

    /// 换信任级别并把接收策略重置为该级别的默认值。
    ///
    /// **带上当前策略**：用户显式设过的保存位置与代收授权要留住，理由见
    /// [`DeviceReceivePolicy::for_trust_level`]。此前这里传的是「没有上一份」，
    /// 于是「升到本人设备」会顺手把自动接收落点清掉，而 UI 上那个开关还开着。
    pub fn apply_trust_level_defaults(&mut self, trust_level: DeviceTrustLevel) {
        self.trust_level = trust_level;
        self.receive_policy =
            DeviceReceivePolicy::for_trust_level(trust_level, Some(&self.receive_policy));
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

    /// 用一次**新观测**刷新本条目：只动 `os_info` / `paired_at`，
    /// **保留** `trust_level` / `receive_policy` / `trust_confirmed`。
    ///
    /// 「新观测」指配对成功回调或 identify 刷新交上来的 [`Self::new`] 产物 —— 它恒为默认
    /// 信任级别与默认收件策略。整条替换等于把用户手工设过的值静默重置，而
    /// `receive_policy` 是被 `swarmdrop_transfer::policy` **真正裁决**的字段，不是展示项：
    /// `Owned` 掉回 `Collaborator`、收紧过的策略放回默认，都会立刻改变入站 offer 的处置。
    ///
    /// 这条规则有两个调用点 —— `paired_devices::upsert`（写库）与
    /// `PairingManager::commit_paired_device` 的**落盘失败回退**（写共享内存表）。
    /// 两边必须是同一份：分叉出来就是「库里守着用户的策略、本次运行却按默认裁决」。
    pub fn merge_observation(&mut self, observed: Self) {
        self.os_info = observed.os_info;
        self.paired_at = observed.paired_at;
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

/// 连接类型：给所有人看的一句话结论。**与 [`PathKind`] 一一对应**，本层不做推断。
///
/// # [`Direct`](Self::Direct) 与 [`Dcutr`](Self::Dcutr) 是两件事，不能合并
///
/// 两者的数据面质量相同（一个字节都不过中继），分开只为回答「怎么建起来的」，
/// 而这两条的排查方向相反：「打洞」意味着 NAT 穿透成功、该去看 ICE 与信令；
/// 「直连」意味着压根没打洞，该去看那条地址是谁给的、隧道底下又是什么。
///
/// 此前 [`PathKind::Direct`] 一档同时装着这两种来路，且被一对一映射成 `Dcutr`，
/// 于是任何非私网非中继的连接都被 UI 标成「打洞」——一条
/// `/ip4/100.x/udp/…/webtransport` 的 Tailscale 直拨会显示成「打洞 + WebTransport」。
/// 现在区分由内核给出（[`PathKind::HolePunched`]），本枚举照搬。
///
/// ⚠️ 那次修复中途走过一条弯路，别再走回去：**不要用「传输是不是 WebRTC」反推打洞**。
/// 原生端 libp2p 自己的 `dcutr` behaviour 是开着的，它打出的是 TCP/QUIC 直连，
/// 按传输反推会把真打洞判成「没打洞」。判据的唯一归属是 [`PathKind`]，
/// 它当前的识别范围与缺口写在那个枚举的文档里。
///
/// [`PathKind`]: swarmdrop_net_base::PathKind
/// [`PathKind::Direct`]: swarmdrop_net_base::PathKind::Direct
/// [`PathKind::HolePunched`]: swarmdrop_net_base::PathKind::HolePunched
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum ConnectionType {
    /// 局域网直连（私网地址或 loopback）。
    Lan,
    /// 直连，但不是打洞来的：公网地址直拨，或经 mesh VPN 隧道（Tailscale 等）。
    Direct,
    /// WebRTC 打洞建立的直连：**信令**经 relay，数据面一个字节不过中继。
    Dcutr,
    /// 经 circuit relay 中继：数据面整条经第三方转发。
    Relay,
}

impl ConnectionType {
    /// 跨 FFI 的 wire 名，**与 serde 的 camelCase 逐字一致**。
    ///
    /// 与 [`TransportKind::wire_name`] 同一体例、同一理由：移动端隔着 uniffi 的
    /// `Option<String>`，此前那份是手抄的。手抄的失败模式格外隐蔽——JS 侧按字符串查表，
    /// 对不上就把整枚徽标收成 `null` 静默丢掉，编得过、跑得动、只是那类连接的设备卡上
    /// 少一块。一致性由 `wire_names_match_serde` 钉死。
    ///
    /// 用 `match` 而非 `_` 分支：加变体时这里编译失败，那正是要的。
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::Lan => "lan",
            Self::Direct => "direct",
            Self::Dcutr => "dcutr",
            Self::Relay => "relay",
        }
    }
}

/// 链路详情：当前连接的可核对事实。
///
/// 与 [`ConnectionType`] 的分工——那个是给所有人看的一句话结论（局域网 / 打洞 /
/// 中继），这个是「凭什么这么说」：走的哪条地址、哪种传输、经不经中继、经的是谁。
/// 三端 UI 把它放在默认折叠的区块里，普通用户看不到，排障时一眼能拿到全部证据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ConnectionDetails {
    /// 承载字节的传输协议。
    ///
    /// `None` 是真实存在的情况，不是缺陷：入站中继连接的 `send_back_addr` 只有
    /// `/p2p/<src>` 一段，地址里没有任何传输信息。呈现层照实显示「未知」。
    pub transport: Option<TransportKind>,
    /// 当前最优连接的远端 multiaddr，原样给出——便于直接粘进 issue 或与日志比对。
    #[cfg_attr(feature = "specta", specta(type = String))]
    pub remote_addr: Addr,
    /// 中转身份：经中继时是那台 relay 的 PeerId，直连为 `None`。
    ///
    /// 「经中继」三个字对排障几乎没用，得说清楚经的是哪一台——自建 relay 还是
    /// 局域网里的 LanHelper，处理方式完全不同。
    #[cfg_attr(feature = "specta", specta(type = Option<String>))]
    pub relay: Option<NodeId>,
}

impl ConnectionDetails {
    /// 由一条连接的远端地址派生。判据全部收口在 [`Addr`] 的谓词上。
    pub fn from_addr(addr: Addr) -> Self {
        Self {
            transport: addr.transport(),
            relay: addr.relay_node_id(),
            remote_addr: addr,
        }
    }
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
    /// 链路详情。仅在线且内核报告过连接地址时有值——离线设备、以及只靠 mDNS
    /// 地址推断出 `connection` 的宽限期内，这里是 `None`（没连接就没有链路可谈）。
    pub connection_details: Option<ConnectionDetails>,
    /// 内核尝试过局域网直连升级但失败了。
    ///
    /// 与 `connection == Relay` 一起看才有意义：那时它把「对端本来就在外网」与
    /// 「对端就在同一网段却连不上」分开。后者呈现层应给出可行动的提示——查防火墙，
    /// 或（浏览器上）允许本地网络访问。
    pub lan_upgrade_failed: bool,
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
/// 公网可路由或隧道→直连，circuit→中继。优先级 LAN > Direct > Relay。
///
/// **这里永远推不出 [`Dcutr`](ConnectionType::Dcutr)**，这是判据决定的而非疏漏：
/// 打洞连接的地址形如 `<relay>/p2p-circuit/webrtc/p2p/<target>`，含 circuit 段，
/// 在第一个分支就被算作中继。要把它认出来必须看**最后一个 circuit 段之后还有没有
/// 传输段**（`Addr::dial_tier` 那套），而本函数只是断连宽限期的回退推断——那时内核
/// 已经不报 `path` 了，与其在这里重造一份易错的地址解析，不如让它落到中继这个
/// 保守结论上。准确的判定在 `path_to_connection`，它拿的是内核报的 `PathKind`。
///
/// 隧道地址（Tailscale 等 mesh VPN 的 `100.64.0.0/10`）归 `Direct`：它既不是私网
/// （`is_private_lan` 只认 RFC1918）也不是公网（`is_public_routable` 显式排除了
/// 共享地址空间），漏掉这一条它会一档都不占，宽限期内徽标凭空消失。
pub fn infer_connection_type(addrs: &[Addr]) -> Option<ConnectionType> {
    if addrs.is_empty() {
        return None;
    }

    let mut has_lan = false;
    let mut has_direct = false;
    let mut has_relay = false;

    for addr in addrs {
        if addr.is_circuit() {
            has_relay = true;
        } else if addr.is_private_lan() || addr.is_loopback() {
            has_lan = true;
        } else if addr.is_public_routable() || addr.is_shared_address_space() {
            has_direct = true;
        }
    }

    if has_lan {
        Some(ConnectionType::Lan)
    } else if has_direct {
        Some(ConnectionType::Direct)
    } else if has_relay {
        Some(ConnectionType::Relay)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use swarmdrop_net_base::SecretKey;

    use super::{DeviceName, DeviceTrustLevel, OsInfo, PairedDeviceInfo};

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

    /// 「name → hostname」这条回退是**三端、收发双向共用**的：接收侧由
    /// `TransferCtrlService` 落进传输记录，发送侧由各壳（Web 的 `send_files`、桌面/移动的
    /// 前端）填同一个字段。分叉的表现是同一台设备在「他发给我的」与「我发给他的」两条记录里
    /// 叫不同的名字，而那种不一致没有任何报错。
    #[test]
    fn display_name_prefers_user_set_name_over_hostname() {
        assert_eq!(
            sample(Some("书房的 Mac"), "Chrome").display_name(),
            "书房的 Mac"
        );
    }

    /// 空串与纯空白都算「没设」：内核允许传空串清空设备名，之后落成 `Some("")` 还是 `None`
    /// 取决于路径，这条回退不该被那个差别绊到（前端 `deviceDisplayName` 同义）。
    #[test]
    fn display_name_treats_blank_as_unset() {
        assert_eq!(sample(None, "Chrome").display_name(), "Chrome");
        assert_eq!(sample(Some(""), "Chrome").display_name(), "Chrome");
        assert_eq!(sample(Some("  "), "Chrome").display_name(), "Chrome");
    }

    /// 两级都空时返回空串，**不在这里编一个占位名**——回落到短 PeerId 是展示层的事
    /// （`@swarmdrop/shared-view` 的 `shortPeerId`）。写死的占位会跟着记录一起落库，
    /// 将来换措辞、换语言都改不动它。
    #[test]
    fn display_name_leaves_the_placeholder_to_the_view_layer() {
        assert_eq!(sample(None, "").display_name(), "");
    }

    #[test]
    fn device_name_rejects_empty_and_blank() {
        assert_eq!(DeviceName::parse(""), None);
        assert_eq!(DeviceName::parse("   \t\n  "), None);
        // 全是被剥掉的字符，剥完也是空
        assert_eq!(DeviceName::parse(";;;"), None);
    }

    #[test]
    fn device_name_truncates_by_char_not_byte() {
        // 41 个中文字：按 byte 截断会切碎 UTF-8 序列（进而 panic），按 char 才对
        let raw = "字".repeat(41);
        let name = DeviceName::parse(&raw).expect("41 个中文字应截断而非拒绝");
        assert_eq!(name.as_str().chars().count(), DeviceName::MAX_CHARS);
        assert_eq!(name.as_str(), "字".repeat(DeviceName::MAX_CHARS));
    }

    #[test]
    fn device_name_strips_separator_and_control_chars() {
        let name = DeviceName::parse("我的电脑; caps=lan-helper\u{7}").expect("非空");
        assert!(!name.as_str().contains(';'), "got: {}", name.as_str());
        assert!(
            !name.as_str().chars().any(char::is_control),
            "got: {}",
            name.as_str()
        );
    }

    #[test]
    fn device_name_parse_is_idempotent() {
        for raw in ["我的电脑", "  书房 Mac ;  ", &"字".repeat(41)] {
            let once = DeviceName::parse(raw).expect("非空");
            let twice = DeviceName::parse(once.as_str()).expect("已归一化的串仍非空");
            assert_eq!(once, twice, "parse 必须幂等: {raw}");
        }
    }

    /// **回归锚点：agent_version 分隔符注入。**
    ///
    /// 这条测试红了，意味着设备名里的 `"; caps=..."` 又能穿过 [`DeviceName::parse`]
    /// 被对端解析成 capability——对端会据此把本机当成局域网协助节点（kad server +
    /// relay 候选）。修法是补归一化，**不是**改断言。
    #[test]
    fn device_name_blocks_agent_version_capability_injection() {
        let injected = DeviceName::parse("我的电脑; caps=lan-helper").expect("非空");
        let info = OsInfo {
            name: Some(injected.into_string()),
            ..sample(None, "MacBook-Pro")
        };

        let agent = info.to_agent_version();
        let parsed = OsInfo::from_agent_version(&agent).expect("parse agent_version");

        assert!(
            parsed.capabilities.is_empty(),
            "设备名不得注入 capability，got: {agent}"
        );
        assert!(!parsed.has_capability(OsInfo::LAN_HELPER_CAPABILITY));
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
            let policy = super::DeviceReceivePolicy::for_trust_level(level, None);
            assert!(
                !policy.allow_mcp_accept_from_device,
                "代收默认应关闭: {level:?}"
            );
        }
    }

    /// 切换信任级别时，用户**显式设过**的两项要带过去。
    ///
    /// 这条规则此前只活在桌面与移动各自的 JS 副本里（且两份还不一样），内核这一份一个都不
    /// 保留——于是同一个产品动作有两种行为。规则收进内核后，这里是它唯一的守卫。
    ///
    /// `default_save_location` 尤其要紧：它为空时消费方一律退回手动确认，所以丢了它等于
    /// 「升到本人设备」把自动接收静默关掉，而 UI 上那个开关还开着。
    #[test]
    fn switching_trust_level_preserves_user_set_fields() {
        let previous = super::DeviceReceivePolicy {
            default_save_location: Some("/Users/me/Downloads".to_string()),
            allow_mcp_accept_from_device: true,
            ..super::DeviceReceivePolicy::for_trust_level(DeviceTrustLevel::Collaborator, None)
        };

        for level in [
            DeviceTrustLevel::Owned,
            DeviceTrustLevel::Collaborator,
            DeviceTrustLevel::Temporary,
        ] {
            let policy = super::DeviceReceivePolicy::for_trust_level(level, Some(&previous));
            assert_eq!(
                policy.default_save_location.as_deref(),
                Some("/Users/me/Downloads"),
                "保存位置应带过去: {level:?}"
            );
            assert!(
                policy.allow_mcp_accept_from_device,
                "代收授权应带过去: {level:?}"
            );
        }
    }

    /// `Blocked` 是唯一例外：两项都清零。
    ///
    /// 「已阻止」必须是不留后门的终态——保留一个自动落点或一份代收授权，都会让「阻止」
    /// 这个词名不副实。
    #[test]
    fn blocking_clears_preserved_fields() {
        let previous = super::DeviceReceivePolicy {
            default_save_location: Some("/Users/me/Downloads".to_string()),
            allow_mcp_accept_from_device: true,
            ..super::DeviceReceivePolicy::for_trust_level(DeviceTrustLevel::Owned, None)
        };

        let policy =
            super::DeviceReceivePolicy::for_trust_level(DeviceTrustLevel::Blocked, Some(&previous));
        assert_eq!(policy.default_save_location, None, "阻止后不该留下自动落点");
        assert!(
            !policy.allow_mcp_accept_from_device,
            "阻止后不该留下代收授权"
        );
        assert_eq!(policy.max_transfer_bytes, Some(0), "阻止即拒收一切");
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

    /// 地址推断分三档，且**永远推不出打洞**——判据见 `infer_connection_type` 的文档。
    ///
    /// 隧道那条（`100.64.0.0/10`）尤其要钉：它既不是 RFC1918 私网也不是公网可路由，
    /// 少了 `is_shared_address_space` 那半个条件就一档都不占，宽限期内徽标凭空消失。
    #[test]
    fn address_inference_covers_lan_direct_tunnel_and_relay() {
        use super::{ConnectionType, infer_connection_type};
        let addrs = |s: &str| vec![s.parse::<swarmdrop_net_base::Addr>().expect("合法地址")];

        assert_eq!(
            infer_connection_type(&addrs("/ip4/192.168.1.5/tcp/4001")),
            Some(ConnectionType::Lan)
        );
        assert_eq!(
            infer_connection_type(&addrs("/ip4/47.115.172.218/udp/4001/quic-v1")),
            Some(ConnectionType::Direct),
            "公网直拨是直连，不是打洞"
        );
        assert_eq!(
            infer_connection_type(&addrs("/ip4/100.112.160.47/udp/62829/quic-v1")),
            Some(ConnectionType::Direct),
            "Tailscale 隧道地址必须落进某一档，否则徽标会整枚消失"
        );
        assert_eq!(
            infer_connection_type(&addrs(
                "/ip4/1.2.3.4/tcp/4001/p2p/12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp/p2p-circuit"
            )),
            Some(ConnectionType::Relay)
        );

        assert_eq!(infer_connection_type(&[]), None);
    }

    /// `wire_name` 与 serde 表示必须逐字一致——它是移动端 FFI 那份字符串的事实源。
    #[test]
    fn connection_type_wire_names_match_serde() {
        use super::ConnectionType;
        for kind in [
            ConnectionType::Lan,
            ConnectionType::Direct,
            ConnectionType::Dcutr,
            ConnectionType::Relay,
        ] {
            assert_eq!(
                serde_json::to_string(&kind).unwrap(),
                format!("\"{}\"", kind.wire_name()),
                "{kind:?} 的 wire_name 与 serde 表示不一致"
            );
        }
    }

    /// 局域网优先级最高：同时看到私网与公网地址时按局域网算。
    #[test]
    fn address_inference_prefers_lan_over_direct() {
        use super::{ConnectionType, infer_connection_type};
        let addrs: Vec<swarmdrop_net_base::Addr> =
            ["/ip4/47.115.172.218/tcp/4001", "/ip4/192.168.1.5/tcp/4001"]
                .iter()
                .map(|s| s.parse().expect("合法地址"))
                .collect();

        assert_eq!(
            infer_connection_type(&addrs),
            Some(ConnectionType::Lan),
            "优先级 LAN > Direct > Relay"
        );
    }
}
