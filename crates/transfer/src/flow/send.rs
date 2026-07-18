//! 发送方生命周期：发起 Offer / 暂停 / 取消 / send_actor 访问。
//!
//! 与 `receive` 模块对称；公共结构体定义仍在 [`crate::manager`]。

use std::sync::Arc;
use std::time::Duration;

use swarmdrop_net::{CallOptions, NodeId};
use tracing::{info, warn};
use uuid::Uuid;

/// Offer 请求的响应等待窗口。
///
/// 接收端 `RequireConfirmation` 的 offer 需要人 / MCP agent 做决定。新内核 RPC 的
/// [`CallOptions::timeout`] 是调用方整体超时（open + 写请求 + 等响应），handler 可在此
/// 窗口内长 await 用户决策。接收端 `PENDING_OFFER_TIMEOUT_SECS` 须**小于**它，保证
/// pending 先于发送端放弃被清理，避免"接收端刚接受、发送端已超时"竞态。
const OFFER_RESPONSE_TIMEOUT_SECS: u64 = 180;

use crate::actor::sender::SenderActor;
use crate::coordinator::{ActorReport, CoordinatorInput, NetworkSignal, TransferState};
use crate::events::TransferEvent;
use crate::flow::resume::parse_peer_id;
use crate::manager::{PendingOutboundOffer, PreparedFile, StartSendResult, TransferManager};
use crate::progress::{
    RuntimeTransferDirection, TransferAcceptedEvent, TransferFailedEvent, TransferRejectedEvent,
};
use crate::protocol::{FileInfo, TRANSFER_CTRL, TransferOrigin, TransferRequest, TransferResponse};
use crate::store::CreateSessionInput;
use crate::wire::data_frame::full_fetch_plan;
use crate::{AppError, AppResult};

impl TransferManager {
    /// 发送 Offer 到目标 peer（非阻塞）
    pub async fn send_offer(
        self: &Arc<Self>,
        prepared_id: &Uuid,
        peer_id: &str,
        peer_name: &str,
        selected_file_ids: &[u32],
        origin: TransferOrigin,
    ) -> AppResult<StartSendResult> {
        let prepared = self
            .prepared
            .get(prepared_id)
            .map(|r| r.value().clone())
            .ok_or_else(|| {
                AppError::Transfer(format!("PreparedTransfer not found: {prepared_id}"))
            })?;

        let selected_prepared: Vec<PreparedFile> = prepared
            .files
            .into_iter()
            .filter(|f| selected_file_ids.contains(&f.file_id))
            .collect();

        if selected_prepared.is_empty() {
            return Err(AppError::Transfer("未选择任何文件".into()));
        }

        let selected_files: Vec<FileInfo> = selected_prepared.iter().map(FileInfo::from).collect();

        let total_size: u64 = selected_files.iter().map(|f| f.size).sum();
        let source_paths: Vec<String> = selected_prepared
            .iter()
            .map(|f| f.source_id.0.clone())
            .collect();
        let session_id = Uuid::new_v4();

        let target_peer = parse_peer_id(peer_id)?;
        let peer_id_str = peer_id.to_string();

        info!(
            "Sending transfer offer to {}: session={}, files={}",
            target_peer,
            session_id,
            selected_files.len()
        );

        self.store
            .create_session(CreateSessionInput {
                session_id,
                direction: entity::TransferDirection::Send,
                peer_id: &peer_id_str,
                peer_name,
                files: &selected_files,
                total_size,
                save_path: None,
                source_paths: Some(&source_paths),
                lifecycle: TransferState::waiting_accept(0),
                policy: None,
                origin: Some(origin.clone()),
            })
            .await?;
        self.coordinator.publish_projection(session_id).await?;
        self.outbound_offers.insert(
            session_id,
            PendingOutboundOffer {
                prepared_id: *prepared_id,
            },
        );

        let endpoint = self.endpoint.clone();
        let this = Arc::clone(self);
        let prepared_id = *prepared_id;
        n0_future::task::spawn(async move {
            let bus = this.events.clone();
            let publish_failed = |error: String| {
                let bus = bus.clone();
                async move {
                    let _ = bus
                        .emit(TransferEvent::TransferFailed {
                            event: TransferFailedEvent {
                                session_id,
                                direction: RuntimeTransferDirection::Send,
                                error,
                            },
                        })
                        .await;
                }
            };

            let result = TRANSFER_CTRL
                .call_with(
                    &endpoint,
                    target_peer,
                    &TransferRequest::Offer {
                        session_id,
                        files: selected_files.clone(),
                        total_size,
                        origin,
                    },
                    CallOptions {
                        timeout: Duration::from_secs(OFFER_RESPONSE_TIMEOUT_SECS),
                    },
                )
                .await;

            match result {
                Ok(TransferResponse::OfferResult { accepted: true, .. }) => {
                    info!("Offer accepted for session {}", session_id);
                    if this.discard_cancelled_outbound_offer(session_id, prepared_id) {
                        this.notify_cancel(target_peer, session_id).await;
                        info!(
                            "Offer accepted after local cancel, sent Cancel: session={session_id}"
                        );
                        return;
                    }

                    let send_actor = Arc::new(SenderActor::new(
                        session_id,
                        target_peer,
                        selected_prepared,
                        this.file_access.clone(),
                        this.events.clone(),
                    ));
                    this.insert_send_actor(session_id, 0, send_actor);
                    this.close_accepted_outbound_offer(session_id, prepared_id);

                    if this.cancelled_outbound_offers.remove(&session_id).is_some() {
                        if let Some(session) = this.remove_send_actor(&session_id) {
                            session.cancel();
                        }
                        // 本地撤回 → 状态机 User{Cancel}（terminal/cancelled + projection），
                        // 与 cancel_send 同一路径。
                        if let Err(e) = this
                            .coordinator
                            .dispatch(
                                session_id,
                                crate::coordinator::CoordinatorInput::User(
                                    crate::coordinator::UserCommand::Cancel,
                                ),
                            )
                            .await
                        {
                            warn!("dispatch 已撤回发送失败: {}", e);
                        }
                        this.notify_cancel(target_peer, session_id).await;
                        info!(
                            "Send session cancelled immediately after accept: session={session_id}"
                        );
                        return;
                    }

                    if let Err(e) = this
                        .coordinator
                        .dispatch(
                            session_id,
                            CoordinatorInput::Network {
                                epoch: 0,
                                signal: NetworkSignal::OfferAccepted,
                            },
                        )
                        .await
                    {
                        warn!("dispatch OfferAccepted 失败: {}", e);
                    }

                    let _ = this
                        .events
                        .emit(TransferEvent::TransferAccepted {
                            event: TransferAcceptedEvent { session_id },
                        })
                        .await;
                    this.spawn_send_data_channel(session_id, 0, full_fetch_plan(&selected_files));
                }
                Ok(TransferResponse::OfferResult {
                    accepted: false,
                    reason,
                }) => {
                    if this.finish_unaccepted_outbound_offer(session_id, prepared_id) {
                        return;
                    }
                    info!("Offer rejected for session {}: {:?}", session_id, reason);
                    if let Err(e) = this
                        .coordinator
                        .dispatch(
                            session_id,
                            CoordinatorInput::Network {
                                epoch: 0,
                                signal: NetworkSignal::OfferRejected,
                            },
                        )
                        .await
                    {
                        warn!("dispatch OfferRejected 失败: {}", e);
                    }
                    let _ = this
                        .events
                        .emit(TransferEvent::TransferRejected {
                            event: TransferRejectedEvent { session_id, reason },
                        })
                        .await;
                }
                Ok(other) => {
                    if this.finish_unaccepted_outbound_offer(session_id, prepared_id) {
                        return;
                    }
                    warn!("意外的响应类型: {:?}", other);
                    this.mark_offer_fatal(session_id, "意外的响应类型").await;
                    publish_failed(format!("意外的响应类型: {other:?}")).await;
                }
                Err(e) => {
                    if this.finish_unaccepted_outbound_offer(session_id, prepared_id) {
                        return;
                    }
                    warn!("发送 Offer 失败: {}", e);
                    this.mark_offer_fatal(session_id, &format!("发送 Offer 失败: {e}"))
                        .await;
                    publish_failed(format!("发送 Offer 失败: {e}")).await;
                }
            }
        });

        Ok(StartSendResult { session_id })
    }

    pub fn get_send_actor(&self, session_id: &Uuid) -> Option<Arc<SenderActor>> {
        self.actors.get_send(session_id)
    }

    pub fn insert_send_actor(&self, session_id: Uuid, epoch: i64, session: Arc<SenderActor>) {
        self.actors.insert_send(session_id, epoch, session);
    }

    pub fn remove_send_actor(&self, session_id: &Uuid) -> Option<Arc<SenderActor>> {
        self.actors.remove_send(session_id)
    }

    pub async fn pause_send(&self, session_id: &Uuid) -> AppResult<()> {
        let session = self
            .get_send_actor(session_id)
            .ok_or_else(|| AppError::Transfer(format!("发送会话不存在: {session_id}")))?;

        session.cancel();
        let progress = session.get_file_progress();
        // 落库文件级进度即可：projection 的 transferredBytes 直接 SUM 文件级，无需再
        // 手工 sync 到 session 列。
        self.store
            .save_sender_file_progress(*session_id, &progress)
            .await?;
        self.coordinator
            .dispatch(
                *session_id,
                crate::coordinator::CoordinatorInput::User(crate::coordinator::UserCommand::Pause),
            )
            .await?;
        self.remove_send_actor(session_id);
        self.notify_pause(session.peer_id, *session_id).await;

        info!("Send session paused: session={}", session_id);
        Ok(())
    }

    pub async fn cancel_send(&self, session_id: &Uuid) -> AppResult<()> {
        let Some(session) = self.remove_send_actor(session_id) else {
            if let Some(prepared_id) = self.outbound_offers.get(session_id).map(|o| o.prepared_id) {
                self.cancelled_outbound_offers.insert(*session_id);
                self.prepared.remove(&prepared_id);
                info!("Pending outbound offer cancelled: session={}", session_id);
                self.coordinator
                    .dispatch(
                        *session_id,
                        crate::coordinator::CoordinatorInput::User(
                            crate::coordinator::UserCommand::Cancel,
                        ),
                    )
                    .await?;
                return Ok(());
            }

            return Err(AppError::Transfer(format!("发送会话不存在: {session_id}")));
        };

        session.cancel();
        self.notify_cancel(session.peer_id, *session_id).await;

        self.coordinator
            .dispatch(
                *session_id,
                crate::coordinator::CoordinatorInput::User(crate::coordinator::UserCommand::Cancel),
            )
            .await?;
        info!("Send session cancelled: session={}", session_id);
        Ok(())
    }

    fn finish_unaccepted_outbound_offer(&self, session_id: Uuid, prepared_id: Uuid) -> bool {
        let was_cancelled = self.cancelled_outbound_offers.remove(&session_id).is_some();
        self.outbound_offers.remove(&session_id);
        if was_cancelled {
            self.prepared.remove(&prepared_id);
        }
        was_cancelled
    }

    fn discard_cancelled_outbound_offer(&self, session_id: Uuid, prepared_id: Uuid) -> bool {
        let was_cancelled = self.cancelled_outbound_offers.remove(&session_id).is_some();
        if was_cancelled {
            self.outbound_offers.remove(&session_id);
            self.prepared.remove(&prepared_id);
        }
        was_cancelled
    }

    fn close_accepted_outbound_offer(&self, session_id: Uuid, prepared_id: Uuid) {
        self.outbound_offers.remove(&session_id);
        self.prepared.remove(&prepared_id);
    }

    /// 向对端发 Cancel 控制帧（收发两侧共用；失败仅告警）。
    pub(crate) async fn notify_cancel(&self, peer_id: NodeId, session_id: Uuid) {
        if let Err(e) = TRANSFER_CTRL
            .call(
                &self.endpoint,
                peer_id,
                &TransferRequest::Cancel {
                    session_id,
                    reason: "用户取消".into(),
                },
            )
            .await
        {
            warn!("通知对方取消失败: session={}, {}", session_id, e);
        }
    }

    /// 向对端发 Pause 控制帧（收发两侧共用；失败仅告警）。
    pub(crate) async fn notify_pause(&self, peer_id: NodeId, session_id: Uuid) {
        if let Err(e) = TRANSFER_CTRL
            .call(
                &self.endpoint,
                peer_id,
                &TransferRequest::Pause { session_id },
            )
            .await
        {
            warn!("通知对方暂停失败: session={}, {}", session_id, e);
        }
    }

    async fn mark_offer_fatal(&self, session_id: Uuid, message: &str) {
        if let Err(e) = self
            .coordinator
            .dispatch(
                session_id,
                CoordinatorInput::Actor {
                    epoch: 0,
                    report: ActorReport::FatalError(message.into()),
                },
            )
            .await
        {
            warn!("dispatch Offer fatal 失败: session={}, {}", session_id, e);
        }
    }
}
