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
/// `Direct` 为局域网直连（授权依据是「对端在本机 mDNS 多播域内」）；`Invite` 携带
/// 一次性邀请凭证（invite_id + capability，见 [`swarmdrop_invite`]）——受邀方
/// 解码邀请串后连接发起方并出示凭证，发起方按
/// [`InviteRegistry`](swarmdrop_invite::InviteRegistry) 校验。
///
/// 6 位分享码（旧 `Code` 变体 + DHT 发布/查询）已废弃——低熵可枚举、DHT 记录无法
/// 证明身份，被自包含签名邀请取代（openspec: pair-invite-protocol）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum PairingMethod {
    Direct,
    Invite {
        /// 邀请标识（发起端据此查 Registry）。
        invite_id: [u8; 16],
        /// bearer 凭证明文（发起端比对哈希；信道保密靠邀请串的 fragment 传递）。
        capability: [u8; 32],
    },
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
