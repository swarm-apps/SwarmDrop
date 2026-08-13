//! SwarmDrop 配对邀请（PairInvite）——编解码、一次性状态表、二维码生成。
//!
//! 一次性签名邀请取代 6 位配对码（openspec: pair-invite-protocol）。本 crate 是
//! **wasm-clean 的独立层**：只依赖 `swarmdrop-net-base` 的身份/地址类型 + 编码库
//! （sha2/postcard/data-encoding/qrcode），**不依赖 core**——core（`PairingManager`）
//! 与浏览器端（`swarmdrop-web` 受邀方 decode）共享它。
//!
//! 邀请只有**一种**对外文本形态：canonical https 链接 + base32 payload
//! （openspec: invite-url-canonical）。链接分享、二维码、剪贴板感知、深链 payload
//! 四种用法同形，解析统一走 [`PairInvite::decode`]——它是三端唯一入口，
//! 不要在 TS / Kotlin / Swift 里另写一份邀请解析器（信任凭证的解析漂移就是安全问题）。
//!
//! ⚠️ **同形不等于同一个字符串**：二维码承载的那份可能少几条地址提示（[`qr`] 按码面密度
//! 回收，见那里）。两者的 capability、身份、TTL、策略完全相同，所以撤销与一次性消费
//! 分辨不出差别 —— 能这么做的前提是地址提示不在签名覆盖范围内。
//!
//! - [`PairInvite`]：邀请的领域类型 + 签名编解码（`invite` 模块）
//! - [`compose`]：邀请携带哪些地址提示 —— 挑选与回收共用同一份价值序
//! - [`InviteRegistry`]：发起端一次性消费状态表（TTL + capability 哈希 + CAS）
//! - [`qr`]：三端统一的二维码生成（原样编码 + 最优分段 / ECL::M / quiet zone 单点固化）

pub mod compose;
mod invite;
pub mod qr;
pub mod store;

pub use compose::select_invite_addrs;

pub use invite::{
    INVITE_TTL_SECS, InviteParseError, InviteRegistry, InviteRejectReason, InviteSummary,
    NoDialableAddrs, PairInvite, SignedInvite, TransportPolicy,
};
pub use qr::{MIN_PX_PER_MODULE, QrError, invite_qr_matrix, invite_qr_svg, max_modules_for};
pub use store::{
    InviteRecord, InviteStore, NoopInviteStore, PersistedInviteState, capability_hash_from_hex,
    capability_hash_to_hex,
};
