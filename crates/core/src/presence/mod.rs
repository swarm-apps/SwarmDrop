//! 已配对设备 presence（在线状态维持）。
//!
//! 唯一的 presence 大脑：消费连接/ping 事件 + 定时器，维护 per-paired-peer
//! 状态机，并承担在线宣告（DHT OnlineRecord）与连接保活白名单的全部职责。
//!
//! ```text
//! Connected ──断连──▶ Probing(退避重拨, 宽限期内 UI 维持在线)
//!     ▲                  │拨通(PeerConnected)      │宽限超时
//!     └──────────────────┘                         ▼
//!                                        Unreachable(低频 DHT 查在线记录 + 重拨)
//! ```
//!
//! 生命周期由 core 自治：[`PresenceSupervisor::run`] 在
//! [`run_event_loop`](crate::network::event_loop::run_event_loop) 启动时拉起，
//! 随 [`NetManager::shutdown`](crate::network::NetManager::shutdown) 的
//! CancellationToken 结束。host 层无需任何 presence 调用。

mod supervisor;

use serde::{Deserialize, Serialize};
use swarm_p2p_core::libp2p::{Multiaddr, PeerId};

use crate::device::OsInfo;

pub use supervisor::{PresenceMap, PresenceState, PresenceSupervisor, PresenceTimings};

/// 中继提示：对端可先与该 relay 建立连接，再拨本机的 circuit 地址。
///
/// circuit 地址内嵌的 relay 地址可能对跨网对端不可达（如 LAN Helper 的
/// 私网 IP）；hint 提供 relay 的全部已知地址供对端修复这段前置链路。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct RelayHint {
    #[cfg_attr(feature = "specta", specta(type = String))]
    pub peer_id: PeerId,
    #[cfg_attr(feature = "specta", specta(type = Vec<String>))]
    pub addrs: Vec<Multiaddr>,
}

/// 在线宣告记录，发布到 DHT 供已配对设备发现地址。
///
/// 结构化的可达性声明（而非裸 listeners 快照）：
/// - `direct_addrs`：可直拨地址（已剔除 loopback/unspecified/多跳 circuit；
///   私网地址保留——跨子网 LAN 场景可用，跨网拨快速失败无害）
/// - `relay_addrs`：合法一跳 circuit 地址
/// - `relays`：中继提示（≤3），供对端先修 relay 直连再拨 circuit
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct OnlineRecord {
    #[serde(flatten)]
    pub os_info: OsInfo,
    #[serde(default)]
    #[cfg_attr(feature = "specta", specta(type = Vec<String>))]
    pub direct_addrs: Vec<Multiaddr>,
    #[serde(default)]
    #[cfg_attr(feature = "specta", specta(type = Vec<String>))]
    pub relay_addrs: Vec<Multiaddr>,
    #[serde(default)]
    pub relays: Vec<RelayHint>,
    pub timestamp: i64,
}

impl OnlineRecord {
    /// 全部可尝试直拨的地址（direct + circuit）
    pub fn dialable_addrs(&self) -> Vec<Multiaddr> {
        self.direct_addrs
            .iter()
            .chain(self.relay_addrs.iter())
            .cloned()
            .collect()
    }
}

/// OnlineRecord 的 DHT TTL（秒）。宣告刷新周期取其一半。
pub const ONLINE_RECORD_TTL_SECS: u64 = 300;
