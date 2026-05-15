//! 文件传输模块
//!
//! 实现端到端加密的文件传输功能，包括文件分块、进度追踪、收发会话等。
//! `crypto` 已下沉到 [`swarmdrop_core::transfer::crypto`]。

pub mod offer;
pub mod progress;
pub mod receiver;
pub mod sender;
