//! 浏览器邀请列表与预览，凭据明文不出边界。
use serde::Serialize;

/// 「已发出的邀请」列表条目（openspec: invite-persistence）。
///
/// **没有邀请串本身**：capability 明文不落盘也不出注册表，刷新后拼不回原始链接。
/// UI 只显示元数据 + 提供撤销；想再分享就生成一条新的。
///
/// 时间戳用字符串承载 Unix 秒，与 `pendingId` 同一个理由（避免 u64 → BigInt 的取回麻烦）。
#[derive(Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct InviteListItemJson {
    /// `sha256(capability)` 的 hex —— 撤销时回传，UI 当不透明 ID 用。
    pub id: String,
    pub created_at: String,
    pub expires_at: String,
    /// 已被对方消费（仍显示到过期，让用户知道它被用过）。
    pub consumed: bool,
}

/// 邀请串解码后的展示投影（配对确认卡用）。
///
/// **不含 capability**：那是 128bit bearer 凭据，明文绝不出 wasm 边界——确认卡只需要
/// 「这是谁、还有效多久、是不是仅局域网」，多给一个字段就多一条泄漏路径。
///
/// **TTL 由调用方按 `expiresAt` 判**，这里不放 `expired: bool`：确认卡会在屏幕上停留几十秒，
/// 而布尔在序列化那一刻就开始变旧。权威判定本来也在发起端的 `InviteRegistry`，
/// 解码侧的预检只为 UX（见 `PairInvite::decode` 的文档）。
///
/// `expiresAt` 用字符串承载 Unix 秒，与 [`InviteListItemJson`] 同一个理由。
#[derive(Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PairInvitePreviewJson {
    /// 发起方 NodeId（base58）。取自签名覆盖范围内的 `inviter_id`，伪造不了——
    /// 自我过滤 / 已配对过滤都该以它为判据。
    pub peer_id: String,
    pub display_name: String,
    pub display_platform: String,
    pub expires_at: String,
    /// LocalOnly 策略（受邀方只用私网地址、禁公网 fallback）。
    pub local_only: bool,
}
