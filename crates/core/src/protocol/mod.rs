//! 应用层协议类型（wire v2）。
//!
//! 按协议分流后不再有复合 `AppRequest`/`AppResponse` 枚举：pairing 与 transfer
//! 控制面各自是独立的 [`Rpc`](swarmdrop_net::Rpc)，数据面是裸流帧协议。
//!
//! - [`ids`] —— ProtocolId 常量 + 两个 `Rpc` 常量（typed RPC 定义）
//! - [`pairing`] —— 配对请求/响应类型
//! - [`transfer_ctrl`] —— 传输控制面请求/响应类型 + 文件/续传元信息

pub mod ids;
pub mod pairing;
pub mod transfer_ctrl;

pub use ids::{
    IDENTIFY_PROTOCOL, PAIRING, PAIRING_PROTOCOL, TRANSFER_CTRL, TRANSFER_CTRL_PROTOCOL,
    TRANSFER_DATA_PROTOCOL,
};
pub use pairing::{PairingMethod, PairingRefuseReason, PairingRequest, PairingResponse};
pub use transfer_ctrl::{
    FileCheckpoint, FileInfo, FileRange, OfferRejectReason, ResumePhaseReport, ResumeRejectReason,
    ResumeReport, TransferOrigin, TransferRequest, TransferResponse,
};
