//! 入站传输请求的跨宿主分发逻辑 + transfer 控制面 typed RPC 服务。

use std::sync::Arc;

use async_trait::async_trait;
use serde::Serialize;
use swarmdrop_net::{AcceptError, Endpoint, NodeId, PathKind, RpcService};
use tokio::sync::oneshot;
use tracing::warn;
use uuid::Uuid;

use crate::device::PairedDeviceInfo;
use crate::error::AppResult;
use crate::events::{TransferEvent, TransferEventSink};
use crate::host::{CoreSaveLocation, Notification, Notifier};
use crate::manager::TransferManager;
use crate::peer::PeerDirectory;
use crate::policy::{
    ReceivePolicyAction, ReceivePolicyContext, ReceivePolicyDecision, evaluate_receive_policy,
};
use crate::progress::{TransferFailedEvent, TransferPausedEvent};
use crate::protocol::{
    FileInfo, OfferRejectReason, ResumeRejectReason, TransferOrigin, TransferRequest,
    TransferResponse,
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
    /// 发起来源（人工 / MCP 代理），供接收端 UI 标识。
    pub origin: TransferOrigin,
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
    async fn handle_peer_disconnected(&self, peer_id: NodeId) {
        let _ = peer_id;
    }

    /// 是否处于全局「暂停接收」状态。
    ///
    /// 默认 `false`：未实现该开关的平台（如 mobile-core）行为与引入本能力前完全一致。
    /// 暂停**仅**作用于是否接受新的传入文件传输，不影响节点在线 / 配对 / 发现。
    fn is_receiving_paused(&self) -> bool {
        false
    }

    /// 缓存入站 offer，返回一个应答通道接收端：handler await 它拿到用户/自动决策。
    ///
    /// 自动接收路径由 `accept_cached_inbound_offer` 立即 send 决策；手动路径由 UI
    /// accept/reject 时 send。清理回收 → drop sender → handler 得 RecvError → 婉拒。
    #[expect(
        clippy::too_many_arguments,
        reason = "缓存入站 offer 需要完整的对端与会话上下文"
    )]
    async fn cache_inbound_offer(
        &self,
        peer_id: NodeId,
        device_name: String,
        session_id: Uuid,
        files: Vec<FileInfo>,
        total_size: u64,
        origin: TransferOrigin,
        policy_decision: ReceivePolicyDecision,
    ) -> AppResult<oneshot::Receiver<TransferResponse>>;

    async fn accept_cached_inbound_offer(
        &self,
        session_id: Uuid,
        save_location: CoreSaveLocation,
    ) -> AppResult<()>;

    #[expect(
        clippy::too_many_arguments,
        reason = "记录被拒 offer 需完整对端/会话/策略上下文"
    )]
    async fn record_rejected_inbound_offer(
        &self,
        peer_id: NodeId,
        peer_name: String,
        session_id: Uuid,
        files: Vec<FileInfo>,
        total_size: u64,
        origin: TransferOrigin,
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
        peer_id: NodeId,
        session_id: Uuid,
        new_epoch: i64,
        fetch_plan: Vec<crate::protocol::FileRange>,
    ) -> AppResult<TransferResponse> {
        let _ = (peer_id, fetch_plan);
        Ok(TransferResponse::ResumeAck {
            session_id,
            new_epoch,
            accepted: false,
            reason: Some(ResumeRejectReason::SessionNotFound),
        })
    }
}

fn offer_result(accepted: bool, reason: Option<OfferRejectReason>) -> TransferResponse {
    TransferResponse::OfferResult { accepted, reason }
}

/// 处理一个入站传输控制请求，返回应给对端的 [`TransferResponse`]。
///
/// Offer 的「require confirmation」在此内部 await 用户决策（新内核 RPC handler 天然
/// 支持长交互），返回值即最终的 `OfferResult`。
pub async fn handle_incoming_transfer_request<R, B>(
    runtime: &R,
    events: &B,
    notifier: Option<&Arc<dyn Notifier>>,
    peer_id: NodeId,
    paired_device: Option<PairedDeviceInfo>,
    via_relay: bool,
    request: TransferRequest,
) -> AppResult<TransferResponse>
where
    R: IncomingTransferRuntime,
    B: TransferEventSink + ?Sized,
{
    match request {
        TransferRequest::Cancel { session_id, reason } => {
            let event = runtime.handle_cancel(session_id, reason).await?;
            events.emit(TransferEvent::TransferFailed { event }).await?;
            Ok(TransferResponse::Ack { session_id })
        }
        TransferRequest::Pause { session_id } => {
            let event = runtime.handle_pause(session_id).await?;
            events.emit(TransferEvent::TransferPaused { event }).await?;
            Ok(TransferResponse::Ack { session_id })
        }
        TransferRequest::Offer {
            session_id,
            files,
            total_size,
            origin,
        } => {
            let Some(paired_device) = paired_device else {
                return Ok(offer_result(false, Some(OfferRejectReason::NotPaired)));
            };

            // 全局「暂停接收」：节点保持在线可发现，但对新 offer 自动婉拒——
            // 不缓存、不落盘、不发 TransferOffer 事件、不打扰本机用户。恢复后照常处理。
            if runtime.is_receiving_paused() {
                return Ok(offer_result(
                    false,
                    Some(OfferRejectReason::ReceivingPaused),
                ));
            }

            let policy_decision = evaluate_receive_policy(ReceivePolicyContext {
                device: Some(&paired_device),
                files: &files,
                total_size,
                via_relay,
                now_ms: chrono::Utc::now().timestamp_millis(),
            });
            let device_name = display_device_name(&paired_device);

            if policy_decision.action == ReceivePolicyAction::Reject {
                runtime
                    .record_rejected_inbound_offer(
                        peer_id,
                        device_name,
                        session_id,
                        files,
                        total_size,
                        origin,
                        policy_decision,
                    )
                    .await?;
                return Ok(offer_result(false, Some(OfferRejectReason::PolicyRejected)));
            }

            let auto_save_location = policy_decision.save_location.clone();
            let policy_action = Some(policy_decision.action_name().to_string());
            let policy_reason = Some(policy_decision.reason.clone());

            let rx = runtime
                .cache_inbound_offer(
                    peer_id,
                    device_name.clone(),
                    session_id,
                    files.clone(),
                    total_size,
                    origin.clone(),
                    policy_decision,
                )
                .await?;

            if let Some(save_location) = auto_save_location {
                // 自动接收：立即解决应答通道并启动接收
                runtime
                    .accept_cached_inbound_offer(session_id, save_location)
                    .await?;
            } else {
                // 需要确认：发 offer 事件 + 系统通知，随后 await 用户决策
                let offer = TransferOfferEvent {
                    session_id,
                    peer_id: peer_id.to_string(),
                    device_name: device_name.clone(),
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
                    origin,
                    policy_action,
                    policy_reason,
                };
                events
                    .emit(TransferEvent::TransferOfferReceived { offer })
                    .await?;
                if let Some(notifier) = notifier {
                    let _ = notifier
                        .notify_if_unfocused(Notification::IncomingTransfer { device_name })
                        .await;
                }
            }

            // await 决策：清理回收 / accept / reject 都经应答通道解决。
            Ok(rx
                .await
                .unwrap_or_else(|_| offer_result(false, Some(OfferRejectReason::UserDeclined))))
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
            Ok(response)
        }
        TransferRequest::ResumeCommit {
            session_id,
            new_epoch,
            fetch_plan,
        } => {
            let response = runtime
                .handle_resume_commit(peer_id, session_id, new_epoch, fetch_plan)
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
            Ok(response)
        }
    }
}

fn display_device_name(device: &PairedDeviceInfo) -> String {
    device
        .os_info
        .name
        .clone()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| device.os_info.hostname.clone())
}

/// transfer 控制面 typed RPC 服务。
///
/// 从 [`Endpoint`] 的连接快照判定 `via_relay`，从 [`PeerDirectory`] 解析 offer 的
/// 已配对设备，再委托 [`handle_incoming_transfer_request`]。
pub struct TransferCtrlService {
    transfer: Arc<TransferManager>,
    pairing: Arc<dyn PeerDirectory>,
    endpoint: Endpoint,
    notifier: Option<Arc<dyn Notifier>>,
}

impl TransferCtrlService {
    pub fn new(
        transfer: Arc<TransferManager>,
        pairing: Arc<dyn PeerDirectory>,
        endpoint: Endpoint,
        notifier: Option<Arc<dyn Notifier>>,
    ) -> Self {
        Self {
            transfer,
            pairing,
            endpoint,
            notifier,
        }
    }

    /// 与对端当前连接是否走中继（Offer 策略的 `allow_relay_auto_accept` 判定用）。
    fn is_via_relay(&self, from: NodeId) -> bool {
        // with() 只借用不 clone 整个 conns 快照。
        self.endpoint.watch_conns().with(|conns| {
            conns
                .get(&from)
                .is_some_and(|conn| matches!(conn.path, PathKind::Relayed))
        })
    }
}

impl RpcService<TransferRequest, TransferResponse> for TransferCtrlService {
    async fn handle(
        &self,
        from: NodeId,
        req: TransferRequest,
    ) -> Result<TransferResponse, AcceptError> {
        let is_offer = matches!(req, TransferRequest::Offer { .. });
        // Offer 才需要解析已配对设备与中继判定；其余控制请求不依赖它们。
        let paired_device = is_offer
            .then(|| self.pairing.get_paired_device(&from))
            .flatten();
        let via_relay = is_offer && self.is_via_relay(from);

        handle_incoming_transfer_request(
            self.transfer.as_ref(),
            self.transfer.events.as_ref(),
            self.notifier.as_ref(),
            from,
            paired_device,
            via_relay,
            req,
        )
        .await
        .map_err(AcceptError::from_err)
    }
}
