//! 配对协议请求/响应类型（wire v2）。

use serde::{Deserialize, Serialize};

use crate::device::OsInfo;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PairingRequest {
    pub os_info: OsInfo,
    pub timestamp: i64,
    pub method: PairingMethod,
}

/// 配对方式。
///
/// `Code` 携带分享码作凭证；`Direct` 为局域网直连（授权依据是「对端在本机 mDNS
/// 多播域内」）。未来的邀请链接机制会新增 `Invite` 变体（扩展位，预留于此）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum PairingMethod {
    Code { code: String },
    Direct,
}

/// 配对被拒绝的原因。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum PairingRefuseReason {
    UserRejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum PairingResponse {
    Success,
    Refused { reason: PairingRefuseReason },
}
