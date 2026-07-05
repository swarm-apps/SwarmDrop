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
use swarm_p2p_core::libp2p::Multiaddr;

use crate::device::OsInfo;

pub use supervisor::{PresenceMap, PresenceState, PresenceSupervisor, PresenceTimings};

/// 在线宣告记录，发布到 DHT 供已配对设备发现地址。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct OnlineRecord {
    #[serde(flatten)]
    pub os_info: OsInfo,
    #[serde(default)]
    #[cfg_attr(feature = "specta", specta(type = Vec<String>))]
    pub listen_addrs: Vec<Multiaddr>,
    pub timestamp: i64,
}

/// OnlineRecord 的 DHT TTL（秒）。宣告刷新周期取其一半。
pub const ONLINE_RECORD_TTL_SECS: u64 = 300;
