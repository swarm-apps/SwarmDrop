//! 数据面层：帧编解码、data channel 路由、分块加密。
//!
//! - [`data_frame`] —— `TransferDataFrame` 编解码 + manifest digest
//! - [`data_plane`] —— data channel 入站/出站路由到 actor（纯路由 + 注册表簿记）
//! - [`crypto`]     —— XChaCha20-Poly1305 分块加密

pub mod crypto;
pub mod data_frame;
pub(crate) mod data_plane;
