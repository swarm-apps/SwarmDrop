//! 入站传输请求的跨宿主分发逻辑。

use async_trait::async_trait;
use serde::Serialize;
use swarm_p2p_core::libp2p::PeerId;
use tracing::warn;
use uuid::Uuid;

use crate::device::PairedDeviceInfo;
use crate::error::{AppError, AppResult};
use crate::host::{CoreEvent, CoreSaveLocation, EventBus};
use crate::protocol::{
    AppNetClient, AppResponse, FileInfo, OfferRejectReason, ResumeRejectReason, TransferRequest,
    TransferResponse,
};
use crate::transfer::policy::{
    ReceivePolicyAction, ReceivePolicyContext, ReceivePolicyDecision, evaluate_receive_policy,
};
use crate::transfer::progress::{TransferFailedEvent, TransferPausedEvent};

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct TransferOfferEvent {
    pub session_id: Uuid,
    pub peer_id: String,
    pub device_name: String,
    pub files: Vec<TransferOfferFileEvent>,
    pub total_size: u64,
    pub policy_action: Option<String>,
    pub policy_reason: Option<String>,
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

/// 宿主侧传输运行时。
///
/// Core 负责协议分发、响应和标准事件发布；具体的文件会话、DB 和宿主清理
/// 由桌面端或 RN 端在这个 trait 中适配。
#[async_trait]
pub trait IncomingTransferRuntime: Send + Sync {
    async fn handle_cancel(
        &self,
        session_id: Uuid,
        reason: String,
    ) -> AppResult<TransferFailedEvent>;

    async fn handle_pause(&self, session_id: Uuid) -> AppResult<TransferPausedEvent>;

    /// 对端断连：把该 peer 当前所有 active 传输转为 recoverable suspended(Interrupted)。
    /// 默认 no-op（mobile-core 占位）；桌面端 TransferManager 具体实现。
    async fn handle_peer_disconnected(&self, peer_id: PeerId) {
        let _ = peer_id;
    }

    async fn cache_inbound_offer(
        &self,
        pending_id: u64,
        peer_id: PeerId,
        device_name: String,
        session_id: Uuid,
        files: Vec<FileInfo>,
        total_size: u64,
        policy_decision: ReceivePolicyDecision,
    ) -> AppResult<()>;

    async fn accept_cached_inbound_offer(
        &self,
        session_id: Uuid,
        save_location: CoreSaveLocation,
    ) -> AppResult<()>;

    async fn record_rejected_inbound_offer(
        &self,
        peer_id: PeerId,
        peer_name: String,
        session_id: Uuid,
        files: Vec<FileInfo>,
        total_size: u64,
        policy_decision: ReceivePolicyDecision,
    ) -> AppResult<()>;

    /// 恢复探测应答（默认报告 NotFound；桌面端在 TransferManager 具体实现）。
    async fn handle_resume_probe(&self, session_id: Uuid) -> AppResult<TransferResponse> {
        Ok(TransferResponse::ResumeStateReport {
            session_id,
            report: crate::protocol::ResumeReport {
                phase: crate::protocol::ResumePhaseReport::NotFound,
                epoch: 0,
                files: vec![],
                checkpoint: vec![],
                source_fingerprint: None,
                terminal: false,
                terminal_reason: None,
            },
        })
    }

    /// 恢复提交应答（默认拒绝；桌面端在 TransferManager 具体实现）。
    async fn handle_resume_commit(
        &self,
        peer_id: PeerId,
        session_id: Uuid,
        new_epoch: i64,
        key: [u8; 32],
        fetch_plan: Vec<crate::protocol::FileRange>,
    ) -> AppResult<TransferResponse> {
        let _ = (peer_id, key, fetch_plan);
        Ok(TransferResponse::ResumeAck {
            session_id,
            new_epoch,
            accepted: false,
            reason: Some(ResumeRejectReason::SessionNotFound),
        })
    }
}

pub async fn handle_incoming_transfer_request<R, B>(
    client: &AppNetClient,
    runtime: &R,
    event_bus: &B,
    peer_id: PeerId,
    pending_id: u64,
    paired_device: Option<PairedDeviceInfo>,
    via_relay: bool,
    request: TransferRequest,
) -> AppResult<IncomingTransferDisposition>
where
    R: IncomingTransferRuntime,
    B: EventBus + ?Sized,
{
    match request {
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
            if paired_device.is_none() {
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

            let policy_decision = evaluate_receive_policy(ReceivePolicyContext {
                device: paired_device.as_ref(),
                files: &files,
                total_size,
                via_relay,
                now_ms: chrono::Utc::now().timestamp_millis(),
            });
            let device_name = paired_device
                .as_ref()
                .map(display_device_name)
                .unwrap_or_else(|| short_peer_id(&peer_id));

            if policy_decision.action == ReceivePolicyAction::Reject {
                let record_result = runtime
                    .record_rejected_inbound_offer(
                        peer_id,
                        device_name,
                        session_id,
                        files,
                        total_size,
                        policy_decision,
                    )
                    .await;
                send_transfer_response(
                    client,
                    pending_id,
                    TransferResponse::OfferResult {
                        accepted: false,
                        key: None,
                        reason: Some(OfferRejectReason::PolicyRejected),
                    },
                )
                .await?;
                record_result?;
                return Ok(IncomingTransferDisposition::Handled);
            }

            let auto_save_location = policy_decision.save_location.clone();
            let policy_action = Some(policy_decision.action_name().to_string());
            let policy_reason = Some(policy_decision.reason.clone());

            runtime
                .cache_inbound_offer(
                    pending_id,
                    peer_id,
                    device_name.clone(),
                    session_id,
                    files.clone(),
                    total_size,
                    policy_decision,
                )
                .await?;

            if let Some(save_location) = auto_save_location {
                runtime
                    .accept_cached_inbound_offer(session_id, save_location)
                    .await?;
                return Ok(IncomingTransferDisposition::Handled);
            }

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
                policy_action,
                policy_reason,
            };
            event_bus
                .publish(CoreEvent::TransferOfferReceived { offer })
                .await?;
            Ok(IncomingTransferDisposition::OfferRequiresConfirmation)
        }
        TransferRequest::ResumeProbe { session_id } => {
            let response = runtime
                .handle_resume_probe(session_id)
                .await
                .unwrap_or_else(|e| {
                    warn!("ResumeProbe 处理失败: {}", e);
                    TransferResponse::ResumeStateReport {
                        session_id,
                        report: crate::protocol::ResumeReport {
                            phase: crate::protocol::ResumePhaseReport::NotFound,
                            epoch: 0,
                            files: vec![],
                            checkpoint: vec![],
                            source_fingerprint: None,
                            terminal: false,
                            terminal_reason: None,
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
                .handle_resume_commit(peer_id, session_id, new_epoch, key, fetch_plan)
                .await
                .unwrap_or_else(|e| {
                    warn!("ResumeCommit 处理失败: {}", e);
                    TransferResponse::ResumeAck {
                        session_id,
                        new_epoch,
                        accepted: false,
                        reason: Some(ResumeRejectReason::SessionNotFound),
                    }
                });
            send_transfer_response(client, pending_id, response).await?;
            Ok(IncomingTransferDisposition::Handled)
        }
    }
}

pub enum IncomingTransferDisposition {
    Handled,
    OfferRequiresConfirmation,
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

fn display_device_name(device: &PairedDeviceInfo) -> String {
    device
        .os_info
        .name
        .clone()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| device.os_info.hostname.clone())
}

fn short_peer_id(peer_id: &PeerId) -> String {
    let s = peer_id.to_string();
    s[s.len().saturating_sub(8)..].to_string()
}
