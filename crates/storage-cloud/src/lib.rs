//! 原生云发布适配器；P2P 随机写仍由 host-fs 承担。
#![cfg(not(target_arch = "wasm32"))]
mod error;
mod file_access;
mod gdrive;
mod persistence;
mod publish;
mod staging;

pub(crate) use error::StorageResult;
pub use error::{CloudFailure, CloudOperation, CloudRecovery, CloudResult, CloudStorageError};
pub use file_access::CloudFileAccess;
pub use gdrive::GoogleDrivePublisher;
pub use publish::{
    CloudPublisher, ProgressSink, PublishIntent, PublishProgress, PublishReceipt, StagedFile,
};
