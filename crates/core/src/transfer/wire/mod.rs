//! 数据面层：帧编解码 + data channel 路由。
//!
//! - [`data_frame`] —— `TransferDataFrame` 编解码 + manifest digest
//! - [`data_plane`] —— data channel 入站/出站路由到 actor（纯路由 + 注册表簿记）
//!
//! wire v2 删除了应用层分块加密（`crypto` 整文件移除）：Noise/TLS 在途已加密，
//! relay 只见密文，密钥经同一加密信道分发是自引用——数据面直接传明文。
//! 传输层身份即归属证明：数据面 handler 校验 `stream.remote() == session.peer_id`。

pub mod data_frame;
pub(crate) mod data_plane;

pub use data_plane::TransferDataHandler;
