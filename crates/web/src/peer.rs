//! 配对目录端口（[`PeerDirectory`]）的 Web 实现：无配对持久化的合成设备目录。

use swarmdrop_host::device::{OsInfo, PairedDeviceInfo};
use swarmdrop_net::NodeId;
use swarmdrop_transfer::peer::PeerDirectory;

/// 无配对目录：对任意对端返回「陌生、需手动确认」的合成设备。
///
/// incoming.rs 对未配对（`None`）offer 硬拒 `NotPaired`（桌面安全边界），故 Web 无配对时
/// 必须给个 `Some`——Collaborator + `auto_accept=false` → policy RequireConfirmation，
/// 语义正是「陌生设备手动确认」，**不改 transfer**。
pub struct WebPeerDirectory;

impl PeerDirectory for WebPeerDirectory {
    fn get_paired_device(&self, peer_id: &NodeId) -> Option<PairedDeviceInfo> {
        Some(PairedDeviceInfo::new(
            *peer_id,
            OsInfo::unknown_from_peer_id(peer_id),
            js_sys::Date::now() as i64,
        ))
    }
}
