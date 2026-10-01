//! 待接收 offer 的浏览器投影。
use serde::Serialize;
use swarmdrop_transfer::protocol::FileInfo;

/// 挂起 offer 的 JS 投影（`pending_offers()` 返回 `OfferJson[]`）。
#[derive(Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct OfferJson {
    pub session_id: String,
    pub peer_id: String,
    pub peer_name: String,
    pub total_size: u64,
    pub files: Vec<FileInfo>,
}
