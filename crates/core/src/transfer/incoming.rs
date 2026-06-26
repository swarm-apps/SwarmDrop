//! 入站传输请求的跨宿主分发逻辑。

use async_trait::async_trait;
use serde::Serialize;
use swarm_p2p_core::libp2p::PeerId;
use tracing::warn;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::host::{CoreEvent, EventBus};
use crate::protocol::{
    AppNetClient, AppResponse, FileChecksum, FileInfo, OfferRejectReason, TransferRequest,
    TransferResponse,
};
use crate::transfer::progress::{
    TransferCompleteEvent, TransferDbErrorEvent, TransferFailedEvent, TransferPausedEvent,
};

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct TransferOfferEvent {
    pub session_id: Uuid,
    pub peer_id: String,
    pub device_name: String,
    pub files: Vec<TransferOfferFileEvent>,
    pub total_size: u64,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct TransferOfferFileEvent {
    pub file_id: u32,
    pub name: String,
    pub relative_path: String,
    pub size: u64,
    pub is_directory: bool,
}

pub struct TransferCompleteOutcome {
    pub event: TransferCompleteEvent,
    pub db_error: Option<TransferDbErrorEvent>,
}

/// 宿主侧传输运行时。
///
/// Core 负责协议分发、响应和标准事件发布；具体的文件会话、DB 和宿主清理
/// 由桌面端或 RN 端在这个 trait 中适配。
#[async_trait]
pub trait IncomingTransferRuntime: Send + Sync {
    async fn handle_chunk_request(
        &self,
        session_id: Uuid,
        file_id: u32,
        chunk_index: u32,
    ) -> AppResult<TransferResponse>;

    async fn handle_complete(&self, session_id: Uuid) -> AppResult<TransferCompleteOutcome>;

    async fn handle_cancel(
        &self,
        session_id: Uuid,
        reason: String,
    ) -> AppResult<TransferFailedEvent>;

    async fn handle_pause(&self, session_id: Uuid) -> AppResult<TransferPausedEvent>;

    fn is_paired(&self, peer_id: &PeerId) -> bool;

    fn paired_device_name(&self, peer_id: &PeerId) -> Option<String>;

    /// 对端断连：把该 peer 当前所有 active 传输转为 recoverable suspended(Interrupted)。
    /// 默认 no-op（mobile-core 占位）；桌面端 TransferManager 具体实现。
    async fn handle_peer_disconnected(&self, peer_id: PeerId) {
        let _ = peer_id;
    }

    fn cache_inbound_offer(
        &self,
        pending_id: u64,
        peer_id: PeerId,
        device_name: String,
        session_id: Uuid,
        files: Vec<FileInfo>,
        total_size: u64,
    );

    /// 接收方发起的断点续传：发送方一侧验证文件 + 重建 SendSession，回复 ResumeResult。
    /// 默认拒绝（mobile-core 占位实现可继承），桌面端在 TransferManager 中具体实现。
    async fn handle_resume_request(
        &self,
        peer_id: PeerId,
        session_id: Uuid,
        file_checksums: Vec<FileChecksum>,
    ) -> AppResult<TransferResponse> {
        let _ = (peer_id, file_checksums);
        Ok(TransferResponse::ResumeResult {
            session_id,
            accepted: false,
            reason: Some(crate::protocol::ResumeRejectReason::SessionNotFound),
            key: None,
        })
    }

    /// 发送方发起的断点续传：接收方一侧验证文件 + 重建 ReceiveSession，回复 ResumeOfferResult。
    async fn handle_resume_offer(
        &self,
        peer_id: PeerId,
        session_id: Uuid,
        key: [u8; 32],
        file_checksums: Vec<FileChecksum>,
    ) -> AppResult<TransferResponse> {
        let _ = (peer_id, key, file_checksums);
        Ok(TransferResponse::ResumeOfferResult {
            session_id,
            accepted: false,
            reason: Some(crate::protocol::ResumeRejectReason::SessionNotFound),
        })
    }

    /// 恢复探测应答（默认报告 NotFound；桌面端在 TransferManager 具体实现）。
    async fn handle_resume_probe(
        &self,
        session_id: Uuid,
        local_epoch: i64,
    ) -> AppResult<TransferResponse> {
        let _ = local_epoch;
        Ok(TransferResponse::ResumeStateReport {
            session_id,
            report: crate::protocol::ResumeReport {
                phase: crate::protocol::ResumePhaseReport::NotFound,
                epoch: 0,
                checkpoint: vec![],
                source_fingerprint: None,
                terminal: false,
            },
        })
    }

    /// 恢复提交应答（默认拒绝；桌面端在 TransferManager 具体实现）。
    async fn handle_resume_commit(
        &self,
        session_id: Uuid,
        new_epoch: i64,
        key: [u8; 32],
        fetch_plan: Vec<crate::protocol::FileRange>,
    ) -> AppResult<TransferResponse> {
        let _ = (key, fetch_plan);
        Ok(TransferResponse::ResumeAck {
            session_id,
            new_epoch,
            accepted: false,
        })
    }
}

pub async fn handle_incoming_transfer_request<R, B>(
    client: &AppNetClient,
    runtime: &R,
    event_bus: &B,
    peer_id: PeerId,
    pending_id: u64,
    request: TransferRequest,
) -> AppResult<IncomingTransferDisposition>
where
    R: IncomingTransferRuntime,
    B: EventBus + ?Sized,
{
    match request {
        TransferRequest::ChunkRequest {
            session_id,
            file_id,
            chunk_index,
        } => {
            let response = runtime
                .handle_chunk_request(session_id, file_id, chunk_index)
                .await
                .unwrap_or_else(|e| {
                    warn!("ChunkRequest 处理失败: {}", e);
                    TransferResponse::ChunkError {
                        session_id,
                        file_id,
                        chunk_index,
                        error: e.to_string(),
                    }
                });
            send_transfer_response(client, pending_id, response).await?;
            Ok(IncomingTransferDisposition::Handled)
        }
        TransferRequest::Complete { session_id } => {
            let outcome = runtime.handle_complete(session_id).await?;
            send_transfer_response(client, pending_id, TransferResponse::Ack { session_id })
                .await?;
            if let Some(event) = outcome.db_error {
                event_bus
                    .publish(CoreEvent::TransferDbError { event })
                    .await?;
            }
            event_bus
                .publish(CoreEvent::TransferCompleted {
                    event: outcome.event,
                })
                .await?;
            Ok(IncomingTransferDisposition::Handled)
        }
        TransferRequest::Cancel { session_id, reason } => {
            let event = runtime.handle_cancel(session_id, reason).await?;
            send_transfer_response(client, pending_id, TransferResponse::Ack { session_id })
                .await?;
            event_bus
                .publish(CoreEvent::TransferFailed { event })
                .await?;
            Ok(IncomingTransferDisposition::Handled)
        }
        TransferRequest::Pause { session_id } => {
            let event = runtime.handle_pause(session_id).await?;
            send_transfer_response(client, pending_id, TransferResponse::Ack { session_id })
                .await?;
            event_bus
                .publish(CoreEvent::TransferPaused { event })
                .await?;
            Ok(IncomingTransferDisposition::Handled)
        }
        TransferRequest::Offer {
            session_id,
            files,
            total_size,
        } => {
            if !runtime.is_paired(&peer_id) {
                send_transfer_response(
                    client,
                    pending_id,
                    TransferResponse::OfferResult {
                        accepted: false,
                        key: None,
                        reason: Some(OfferRejectReason::NotPaired),
                    },
                )
                .await?;
                return Ok(IncomingTransferDisposition::Handled);
            }

            let device_name = runtime.paired_device_name(&peer_id).unwrap_or_else(|| {
                let s = peer_id.to_string();
                s[s.len().saturating_sub(8)..].to_string()
            });

            runtime.cache_inbound_offer(
                pending_id,
                peer_id,
                device_name.clone(),
                session_id,
                files.clone(),
                total_size,
            );

            let offer = TransferOfferEvent {
                session_id,
                peer_id: peer_id.to_string(),
                device_name,
                files: files
                    .into_iter()
                    .map(|f| TransferOfferFileEvent {
                        file_id: f.file_id,
                        name: f.name,
                        relative_path: f.relative_path,
                        size: f.size,
                        is_directory: false,
                    })
                    .collect(),
                total_size,
            };
            event_bus
                .publish(CoreEvent::TransferOfferReceived { offer })
                .await?;
            Ok(IncomingTransferDisposition::Handled)
        }
        TransferRequest::ResumeRequest {
            session_id,
            file_checksums,
        } => {
            let response = runtime
                .handle_resume_request(peer_id, session_id, file_checksums)
                .await
                .unwrap_or_else(|e| {
                    warn!("ResumeRequest 处理失败: {}", e);
                    TransferResponse::ResumeResult {
                        session_id,
                        accepted: false,
                        reason: Some(crate::protocol::ResumeRejectReason::SessionNotFound),
                        key: None,
                    }
                });
            send_transfer_response(client, pending_id, response).await?;
            Ok(IncomingTransferDisposition::Handled)
        }
        TransferRequest::ResumeOffer {
            session_id,
            key,
            file_checksums,
        } => {
            let response = runtime
                .handle_resume_offer(peer_id, session_id, key, file_checksums)
                .await
                .unwrap_or_else(|e| {
                    warn!("ResumeOffer 处理失败: {}", e);
                    TransferResponse::ResumeOfferResult {
                        session_id,
                        accepted: false,
                        reason: Some(crate::protocol::ResumeRejectReason::SessionNotFound),
                    }
                });
            send_transfer_response(client, pending_id, response).await?;
            Ok(IncomingTransferDisposition::Handled)
        }
        TransferRequest::ResumeProbe {
            session_id,
            local_epoch,
        } => {
            let response = runtime
                .handle_resume_probe(session_id, local_epoch)
                .await
                .unwrap_or_else(|e| {
                    warn!("ResumeProbe 处理失败: {}", e);
                    TransferResponse::ResumeStateReport {
                        session_id,
                        report: crate::protocol::ResumeReport {
                            phase: crate::protocol::ResumePhaseReport::NotFound,
                            epoch: 0,
                            checkpoint: vec![],
                            source_fingerprint: None,
                            terminal: false,
                        },
                    }
                });
            send_transfer_response(client, pending_id, response).await?;
            Ok(IncomingTransferDisposition::Handled)
        }
        TransferRequest::ResumeCommit {
            session_id,
            new_epoch,
            key,
            fetch_plan,
        } => {
            let response = runtime
                .handle_resume_commit(session_id, new_epoch, key, fetch_plan)
                .await
                .unwrap_or_else(|e| {
                    warn!("ResumeCommit 处理失败: {}", e);
                    TransferResponse::ResumeAck {
                        session_id,
                        new_epoch,
                        accepted: false,
                    }
                });
            send_transfer_response(client, pending_id, response).await?;
            Ok(IncomingTransferDisposition::Handled)
        }
    }
}

pub enum IncomingTransferDisposition {
    Handled,
    Unhandled(TransferRequest),
}

async fn send_transfer_response(
    client: &AppNetClient,
    pending_id: u64,
    response: TransferResponse,
) -> AppResult<()> {
    client
        .send_response(pending_id, AppResponse::Transfer(response))
        .await
        .map_err(AppError::from)
}
