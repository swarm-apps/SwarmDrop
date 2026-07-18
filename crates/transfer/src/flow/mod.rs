//! 生命周期编排层：`TransferManager` 按阶段拆分的方法（公共结构体在 [`super::manager`]）。
//!
//! - [`prepare`] —— 发送方哈希准备
//! - [`send`]    —— 发送方 Offer / 暂停 / 取消
//! - [`receive`] —— 接收方 accept / reject / 暂停 / 取消 + IncomingTransferRuntime 接收 helper
//! - [`resume`]  —— 双侧断点续传 + IncomingTransferRuntime 续传 helper

pub(crate) mod prepare;
pub(crate) mod receive;
pub(crate) mod resume;
pub(crate) mod send;
