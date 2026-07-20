//! SwarmDrop 配对邀请（PairInvite）——编解码、一次性状态表、二维码生成。
//!
//! 一次性签名邀请取代 6 位配对码（openspec: pair-invite-protocol）。本 crate 是
//! **wasm-clean 的独立层**：只依赖 `swarmdrop-net-base` 的身份/地址类型 + 编码库
//! （sha2/postcard/data-encoding/fast_qr），**不依赖 core**——core（`PairingManager`）
//! 与浏览器端（`swarmdrop-web` 受邀方 decode）共享它。
//!
//! - [`PairInvite`]：邀请的领域类型 + 签名编解码（[`invite`] 模块）
//! - [`InviteRegistry`]：发起端一次性消费状态表（TTL + capability 哈希 + CAS）
//! - [`qr`]：三端统一的二维码生成（大写 alphanumeric / ECL::M / quiet zone 单点固化）

mod invite;
pub mod qr;

pub use invite::{
    INVITE_TTL_SECS, InviteParseError, InviteRegistry, InviteRejectReason, PairInvite,
    TransportPolicy,
};
pub use qr::{QrError, invite_qr_matrix, invite_qr_svg};
