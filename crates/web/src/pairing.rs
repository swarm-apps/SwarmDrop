//! 浏览器配对请求与结果的展示协议。
use serde::Serialize;

/// 挂起入站配对请求的 JS 投影（`pending_pairing_requests()` 返回）。
///
/// browser-as-inviter 用：桌面消费浏览器生成的 invite 后，浏览器（邀请方）本机需弹确认。
/// `pendingId` 以字符串承载核心的 u64 pending id（避免 JS number 精度 + serde_wasm_bindgen
/// 对 u64 产 BigInt 的取回麻烦），`respond_pairing_request` 再 parse 回 u64。
#[derive(Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PendingPairingJson {
    pub pending_id: String,
    pub peer_id: String,
    pub device_name: String,
}

/// 对方拒绝配对的原因判别码 —— 内核 `PairingRefuseReason` 的投影，wire 形状逐字相同
/// （`{ type: "user_rejected" }`，与桌面 bindings 里那个类型可互换）。
///
/// **为什么是投影而不是直接用内核类型**：`swarmdrop-core` 在本 crate 里是 wasm-only
/// 依赖（见 Cargo.toml），而本模块 native 也要编（specta 导出跑在 native）。
///
/// **为什么这份重复是安全的**：唯一的构造点是 `node.rs` 里那个**穷尽 match**，内核加一个
/// 拒绝原因就会在那里编译失败。判别码不能靠字符串传 —— 那只会在运行时静默落到兜底分支。
#[derive(Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum PairingRefusedJson {
    UserRejected,
}

/// 一次配对尝试的结果（与桌面 `PairingOutcome` / 移动 `MobilePairingResult` 同构）。
///
/// 两个字段各自表达一种**不能压成错误的结果**：
///
/// - `refused` —— 对方点了拒绝。这是一次完全正常的交互，不是失败。它曾经被包成
///   `WebError::network("邀请方拒绝了配对或配对未成功")`，于是用户看到标题「网络错误」
///   配一句写死的简体中文（英文界面下尤其突兀），而网络其实一切正常。**判别码进结构体，
///   文案归前端**。
/// - `persisted` —— 走到这一步对端已经收到 `Success` 并把本机加进了它的已配对列表，
///   本机此时若报失败，两台设备对同一件事的认知就永久分叉了。真实后果只有一个 ——
///   这台设备刷新/重启后会从本机列表消失（对端仍记着），UI 该照这个说。
#[derive(Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PairingOutcomeJson {
    /// 对方拒绝时的原因判别码；`None` = 配对达成。
    pub refused: Option<PairingRefusedJson>,
    /// 已配对对端的 NodeId（base58）。`refused` 非空时是空串。
    pub peer_id: String,
    /// 设备是否已落盘。`false` = 刷新页面后这台设备会不见（对端仍记着）。
    /// 仅在配对达成时有意义，被拒时恒为 `true`。
    pub persisted: bool,
}
