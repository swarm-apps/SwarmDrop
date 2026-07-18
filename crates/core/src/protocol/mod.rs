//! 应用层协议类型（wire v2）。
//!
//! 按协议分流后不再有复合 `AppRequest`/`AppResponse` 枚举：pairing 与 transfer
//! 控制面各自是独立的 [`Rpc`](swarmdrop_net::Rpc)，数据面是裸流帧协议。
//!
//! - [`ids`] —— 配对 / identify 的 ProtocolId 常量 + `PAIRING` Rpc
//! - [`pairing`] —— 配对请求/响应类型
//!
//! 传输控制面协议类型 + ids（`FileInfo` / `TransferRequest` / `TRANSFER_CTRL` …）迁入
//! [`swarmdrop_transfer::protocol`]，这里 re-export 保持 `crate::protocol::*` 路径不变。

pub mod ids;
pub mod pairing;

pub use ids::{IDENTIFY_PROTOCOL, PAIRING, PAIRING_PROTOCOL};
pub use pairing::{PairingMethod, PairingRefuseReason, PairingRequest, PairingResponse};
pub use swarmdrop_transfer::protocol::{
    FileCheckpoint, FileInfo, FileRange, OfferRejectReason, ResumePhaseReport, ResumeRejectReason,
    ResumeReport, TRANSFER_CTRL, TRANSFER_CTRL_PROTOCOL, TRANSFER_DATA_PROTOCOL, TransferOrigin,
    TransferRequest, TransferResponse,
};
