//! 配对目录端口——解 incoming.rs 对 core 侧配对管理器的直接依赖。
//!
//! transfer 只需「按 NodeId 查已配对设备」一个查询面（offer 自动接受策略据此判定
//! 信任级）。core 侧的配对管理器实现本端口。

use swarmdrop_net::NodeId;

use crate::device::PairedDeviceInfo;

/// 已配对设备目录（只读查询）。
pub trait PeerDirectory: Send + Sync {
    /// 查已配对设备信息（未配对返回 `None`）。
    fn get_paired_device(&self, peer_id: &NodeId) -> Option<PairedDeviceInfo>;
}
