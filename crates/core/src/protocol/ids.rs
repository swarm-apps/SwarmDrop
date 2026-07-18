//! 协议标识与 typed RPC 定义（wire v2）。
//!
//! 协议名整串精确匹配、无版本协商回退，故版本号进协议名（`/swarmdrop/pairing/2`）。
//! wire v2 相对 v1 不向后兼容（推倒重来）。

use swarmdrop_net::{ProtocolId, Rpc};

use super::pairing::{PairingRequest, PairingResponse};
use super::transfer_ctrl::{TransferRequest, TransferResponse};

/// identify 的 `protocol_version`（协议兼容性检查用）。
pub const IDENTIFY_PROTOCOL: &str = "/swarmdrop/2.0.0";

/// 配对控制协议名。
pub const PAIRING_PROTOCOL: ProtocolId = ProtocolId::from_static("/swarmdrop/pairing/2");

/// 传输控制面协议名。
pub const TRANSFER_CTRL_PROTOCOL: ProtocolId =
    ProtocolId::from_static("/swarmdrop/transfer-ctrl/2");

/// 传输数据面协议名（裸流 + 自带帧协议，见 [`transfer::wire`](crate::transfer::wire)）。
pub const TRANSFER_DATA_PROTOCOL: ProtocolId =
    ProtocolId::from_static("/swarmdrop/transfer-data/2");

/// 配对 typed RPC：`PairingRequest → PairingResponse`。
///
/// handler 可在回复前 await 用户决策（新内核 RPC 天然支持长交互）。
pub const PAIRING: Rpc<PairingRequest, PairingResponse> = Rpc::new(PAIRING_PROTOCOL);

/// 传输控制面 typed RPC：`TransferRequest → TransferResponse`。
pub const TRANSFER_CTRL: Rpc<TransferRequest, TransferResponse> = Rpc::new(TRANSFER_CTRL_PROTOCOL);
