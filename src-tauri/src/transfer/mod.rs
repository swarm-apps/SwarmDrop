//! 文件传输 —— 完全下沉到 [`swarmdrop_core::transfer`]。
//!
//! src-tauri 侧只通过 [`commands::transfer`](crate::commands::transfer) 提供
//! Tauri IPC 命令薄壳，调用 core 的 [`TransferManager`](swarmdrop_core::transfer::manager::TransferManager)。
