//! 运行时 actor 层：单会话执行（发送 / 接收）、actor 注册表、checkpoint 纯函数。
//!
//! - [`sender`] —— `SendSession`：经 data-channel 推送文件块
//! - [`receiver`] —— `ReceiveSession`：落盘 + 断点续传 checkpoint
//! - [`registry`] —— `ActorRegistry`：actor 内存生命周期 + epoch 准入
//! - [`checkpoint`] —— bitmap / range 纯函数（receiver + resume 共用）

pub(crate) mod checkpoint;
pub mod receiver;
pub(crate) mod registry;
pub mod sender;
