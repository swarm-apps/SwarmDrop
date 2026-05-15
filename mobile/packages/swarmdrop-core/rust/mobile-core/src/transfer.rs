//! 文件传输 —— 委托给 [`swarmdrop_core::transfer::manager::TransferManager`]。
//!
//! mobile-core 这层只做：
//! - uniffi Record 镜像（输入/输出类型）
//! - String <-> Uuid / PeerId / SaveLocation 等 ID 转换
//!
//! 业务逻辑（hash、加密、断点续传、DB checkpoint）全部在 core，
//! 文件 I/O 通过 `ForeignFileAccess` callback 走 RN 的 expo-file-system。

use swarmdrop_core::transfer::incoming::{TransferOfferEvent, TransferOfferFileEvent};
use swarmdrop_core::transfer::HostEnumeratedFile;
use uuid::Uuid;

use crate::app::MobileCore;
use crate::error::{FfiError, FfiResult};
use crate::utils::parse_peer_id;

// ─────────────── 输入/输出 Record ───────────────

/// 待发送的单个文件（RN 调 prepare_send 时构造）
#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileTransferFile {
    /// host 自定义的 source 标识（RN 用 expo-fs uri）
    pub source_id: String,
    pub name: String,
    pub relative_path: String,
    pub size: u64,
}

/// 接收方收到的 Offer 中的单个文件
#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileTransferOfferFile {
    pub file_id: u32,
    pub name: String,
    pub relative_path: Option<String>,
    pub size: u64,
    pub is_directory: bool,
}

impl From<TransferOfferFileEvent> for MobileTransferOfferFile {
    fn from(file: TransferOfferFileEvent) -> Self {
        Self {
            file_id: file.file_id,
            name: file.name,
            // core 用空字符串表示根目录文件；FFI 收敛到 Option
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

/// prepare_send 的返回类型
#[derive(Debug, Clone, uniffi::Record)]
pub struct MobilePreparedTransfer {
    pub prepared_id: String,
    pub total_size: u64,
    pub files: Vec<MobilePreparedFile>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobilePreparedFile {
    pub file_id: u32,
    pub name: String,
    pub relative_path: String,
    pub size: u64,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileSendResult {
    pub session_id: String,
}

// ─────────────── MobileCore 方法 ───────────────

#[uniffi::export(async_runtime = "tokio")]
impl MobileCore {
    /// 准备发送：流式 BLAKE3 hash + 通过 EventBus 推 PrepareProgress 事件
    pub async fn prepare_send(
        &self,
        files: Vec<MobileTransferFile>,
    ) -> FfiResult<MobilePreparedTransfer> {
        if files.is_empty() {
            return Err(FfiError::Transfer("no files selected".into()));
        }
        let manager = self.transfer_manager_arc().await?;

        let prepared_id = Uuid::new_v4();
        let host_files: Vec<HostEnumeratedFile> = files
            .into_iter()
            .map(|f| HostEnumeratedFile {
                source_id: swarmdrop_core::host::FileSourceId(f.source_id),
                name: f.name,
                relative_path: f.relative_path,
                size: f.size,
            })
            .collect();

        let prepared = manager.prepare(prepared_id, host_files).await?;
        Ok(MobilePreparedTransfer {
            prepared_id: prepared.prepared_id.to_string(),
            total_size: prepared.total_size,
            files: prepared
                .files
                .iter()
                .map(|f| MobilePreparedFile {
                    file_id: f.file_id,
                    name: f.name.clone(),
                    relative_path: f.relative_path.clone(),
                    size: f.size,
                })
                .collect(),
        })
    }

    /// 发送：构造 Offer 给对端（异步，结果通过 TransferAccepted/Rejected/Failed 事件回报）
    pub async fn send_prepared(
        &self,
        prepared_id: String,
        peer_id: String,
        peer_name: String,
        file_ids: Vec<u32>,
    ) -> FfiResult<MobileSendResult> {
        parse_peer_id(&peer_id)?;
        let prepared_uuid = Uuid::parse_str(&prepared_id)
            .map_err(|_| FfiError::Transfer(format!("invalid prepared_id: {prepared_id}")))?;
        let manager = self.transfer_manager_arc().await?;
        let result = manager
            .send_offer(&prepared_uuid, &peer_id, &peer_name, &file_ids)
            .map_err(FfiError::from)?;
        Ok(MobileSendResult {
            session_id: result.session_id.to_string(),
        })
    }

    /// 接受接收：保存到 RN 提供的目录
    ///
    /// `save_location_uri` 是 RN 端把文件写到哪的标识（比如 expo-fs 的 documentDirectory）。
    /// 实际写入路径由 ForeignFileAccess::create_sink 实现决定，core 只把 uri 当作 SaveLocation::Path 透传。
    pub async fn accept_receive(
        &self,
        session_id: String,
        save_location_uri: String,
    ) -> FfiResult<()> {
        let session_uuid = parse_session_id(&session_id)?;
        let manager = self.transfer_manager_arc().await?;
        manager
            .accept_and_start_receive(
                &session_uuid,
                swarmdrop_core::host::CoreSaveLocation::Path {
                    path: save_location_uri,
                },
            )
            .await
            .map_err(FfiError::from)
    }

    pub async fn reject_receive(&self, session_id: String) -> FfiResult<()> {
        let session_uuid = parse_session_id(&session_id)?;
        let manager = self.transfer_manager_arc().await?;
        manager
            .reject_and_respond(&session_uuid)
            .await
            .map_err(FfiError::from)
    }

    /// 取消传输（自动判断是发送会话还是接收会话）
    pub async fn cancel_transfer(&self, session_id: String) -> FfiResult<()> {
        let session_uuid = parse_session_id(&session_id)?;
        let manager = self.transfer_manager_arc().await?;
        if manager.cancel_send(&session_uuid).await.is_err() {
            let _ = manager.cancel_receive(&session_uuid).await;
        }
        Ok(())
    }

    /// 暂停传输（自动判断方向）
    pub async fn pause_transfer(&self, session_id: String) -> FfiResult<()> {
        let session_uuid = parse_session_id(&session_id)?;
        let manager = self.transfer_manager_arc().await?;
        if manager.pause_send(&session_uuid).await.is_err() {
            let _ = manager.pause_receive(&session_uuid).await;
        }
        Ok(())
    }
}

fn parse_session_id(s: &str) -> FfiResult<Uuid> {
    Uuid::parse_str(s).map_err(|_| FfiError::Transfer(format!("invalid session_id: {s}")))
}
