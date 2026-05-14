//! 文件传输 —— prepared transfer 池 + active sessions 列表。
//!
//! 当前实现是占位骨架:`prepare_send`/`send_prepared` 维护内存中状态机,
//! 真正的分块/加密/落盘逻辑还没有接进来(见 dev-notes/roadmap/phase-3.md)。
//! 状态机命名沿用桌面端的字符串:`waitingForRuntime` / `accepted` / `rejected` /
//! `cancelled` —— RN 侧根据这些字符串做 UI 状态展示。

use std::collections::HashMap;

use swarmdrop_core::transfer::incoming::{TransferOfferEvent, TransferOfferFileEvent};
use uuid::Uuid;

use crate::app::MobileCore;
use crate::error::{FfiError, FfiResult};
use crate::utils::parse_peer_id;

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileTransferFile {
    pub file_id: String,
    pub name: String,
    pub relative_path: Option<String>,
    pub uri: String,
    pub size: u64,
    pub is_directory: bool,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileTransferOfferFile {
    pub file_id: String,
    pub name: String,
    pub relative_path: Option<String>,
    pub size: u64,
    pub is_directory: bool,
}

impl From<TransferOfferFileEvent> for MobileTransferOfferFile {
    fn from(file: TransferOfferFileEvent) -> Self {
        Self {
            file_id: file.file_id.to_string(),
            name: file.name,
            // core 用空字符串表示根目录文件;FFI 把它收敛到 Option
            relative_path: if file.relative_path.is_empty() {
                None
            } else {
                Some(file.relative_path)
            },
            size: file.size,
            is_directory: file.is_directory,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileTransferOffer {
    pub session_id: String,
    pub peer_id: String,
    pub device_name: String,
    pub files: Vec<MobileTransferOfferFile>,
    pub total_size: u64,
}

impl From<TransferOfferEvent> for MobileTransferOffer {
    fn from(offer: TransferOfferEvent) -> Self {
        Self {
            session_id: offer.session_id.to_string(),
            peer_id: offer.peer_id,
            device_name: offer.device_name,
            files: offer.files.into_iter().map(Into::into).collect(),
            total_size: offer.total_size,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobilePreparedTransfer {
    pub prepared_id: String,
    pub total_size: u64,
    pub files: Vec<MobileTransferFile>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileTransferSession {
    pub session_id: String,
    pub peer_id: String,
    pub direction: String,
    pub status: String,
    pub total_size: u64,
    pub completed_size: u64,
    pub progress: f32,
}

#[derive(Default)]
pub(crate) struct MobileTransferState {
    pub(crate) prepared: HashMap<String, MobilePreparedTransfer>,
    pub(crate) sessions: HashMap<String, MobileTransferSession>,
}

#[uniffi::export(async_runtime = "tokio")]
impl MobileCore {
    pub async fn transfer_sessions(&self) -> Vec<MobileTransferSession> {
        self.transfer_state()
            .await
            .sessions
            .values()
            .cloned()
            .collect()
    }

    pub async fn transfer_session(&self, session_id: String) -> Option<MobileTransferSession> {
        self.transfer_state()
            .await
            .sessions
            .get(&session_id)
            .cloned()
    }

    pub async fn prepare_send(
        &self,
        files: Vec<MobileTransferFile>,
    ) -> FfiResult<MobilePreparedTransfer> {
        if files.is_empty() {
            return Err(FfiError::Transfer("no files selected".to_string()));
        }

        let total_size = files.iter().map(|file| file.size).sum();
        let prepared = MobilePreparedTransfer {
            prepared_id: Uuid::new_v4().to_string(),
            total_size,
            files,
        };
        self.transfer_state_mut()
            .await
            .prepared
            .insert(prepared.prepared_id.clone(), prepared.clone());
        Ok(prepared)
    }

    pub async fn send_prepared(
        &self,
        prepared_id: String,
        peer_id: String,
        file_ids: Vec<String>,
    ) -> FfiResult<MobileTransferSession> {
        parse_peer_id(&peer_id)?;
        if self.net_manager_guard().await.is_none() {
            return Err(FfiError::NodeNotStarted);
        }

        let mut state = self.transfer_state_mut().await;
        let prepared = state
            .prepared
            .get(&prepared_id)
            .cloned()
            .ok_or_else(|| FfiError::Transfer("prepared transfer not found".to_string()))?;
        let selected_files: Vec<_> = if file_ids.is_empty() {
            prepared.files
        } else {
            prepared
                .files
                .into_iter()
                .filter(|file| file_ids.contains(&file.file_id))
                .collect()
        };
        if selected_files.is_empty() {
            return Err(FfiError::Transfer("no matching files selected".to_string()));
        }

        let total_size = selected_files.iter().map(|file| file.size).sum();
        let session = MobileTransferSession {
            session_id: Uuid::new_v4().to_string(),
            peer_id,
            direction: "outgoing".to_string(),
            status: "waitingForRuntime".to_string(),
            total_size,
            completed_size: 0,
            progress: 0.0,
        };
        state
            .sessions
            .insert(session.session_id.clone(), session.clone());
        Ok(session)
    }

    pub async fn accept_receive(
        &self,
        session_id: String,
        _destination_uri: String,
    ) -> FfiResult<MobileTransferSession> {
        if self.net_manager_guard().await.is_none() {
            return Err(FfiError::NodeNotStarted);
        }
        let mut state = self.transfer_state_mut().await;
        let session = state
            .sessions
            .get_mut(&session_id)
            .ok_or_else(|| FfiError::Transfer("transfer session not found".to_string()))?;
        session.status = "accepted".to_string();
        Ok(session.clone())
    }

    pub async fn reject_receive(&self, session_id: String) -> FfiResult<()> {
        let mut state = self.transfer_state_mut().await;
        let session = state
            .sessions
            .get_mut(&session_id)
            .ok_or_else(|| FfiError::Transfer("transfer session not found".to_string()))?;
        session.status = "rejected".to_string();
        Ok(())
    }

    pub async fn cancel_transfer(&self, session_id: String) -> FfiResult<()> {
        let mut state = self.transfer_state_mut().await;
        let session = state
            .sessions
            .get_mut(&session_id)
            .ok_or_else(|| FfiError::Transfer("transfer session not found".to_string()))?;
        session.status = "cancelled".to_string();
        Ok(())
    }
}
