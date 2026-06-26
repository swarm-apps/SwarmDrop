//! 发送方生命周期：发起 Offer / 暂停 / 取消 / send_session 访问。
//!
//! 与 `receive` 模块对称；公共结构体定义仍在 [`super::manager`]。

use std::sync::Arc;

use swarm_p2p_core::libp2p::PeerId;
use tracing::{info, warn};
use uuid::Uuid;

use crate::host::CoreEvent;
use crate::protocol::{AppRequest, AppResponse, FileInfo, TransferRequest, TransferResponse};
use crate::transfer::manager::{
    generate_id, PendingOutboundOffer, PreparedFile, StartSendResult, TransferManager,
};
use crate::transfer::progress::{
    RuntimeTransferDirection, TransferAcceptedEvent, TransferDbErrorEvent, TransferFailedEvent,
    TransferRejectedEvent,
};
use crate::transfer::sender::SendSession;
use crate::{AppError, AppResult};

impl TransferManager {
    /// 发送 Offer 到目标 peer（非阻塞）
    pub fn send_offer(
        self: &Arc<Self>,
        prepared_id: &Uuid,
        peer_id: &str,
        peer_name: &str,
        selected_file_ids: &[u32],
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

        let selected_files: Vec<FileInfo> = selected_prepared
            .iter()
            .map(|f| FileInfo {
                file_id: f.file_id,
                name: f.name.clone(),
                relative_path: f.relative_path.clone(),
                size: f.size,
                checksum: f.checksum.clone(),
            })
            .collect();

        let total_size: u64 = selected_files.iter().map(|f| f.size).sum();
        let source_paths: Vec<String> = selected_prepared
            .iter()
            .map(|f| f.source_id.0.clone())
            .collect();
        let session_id = generate_id();

        let target_peer: PeerId = peer_id
            .parse()
            .map_err(|_| AppError::Transfer(format!("无效的 PeerId: {peer_id}")))?;

        info!(
            "Sending transfer offer to {}: session={}, files={}",
            target_peer,
            session_id,
            selected_files.len()
        );

        self.outbound_offers.insert(
            session_id,
            PendingOutboundOffer {
                prepared_id: *prepared_id,
            },
        );

        let client = self.client.clone();
        let this = Arc::clone(self);
        let prepared_id = *prepared_id;
        let peer_id_str = peer_id.to_string();
        let peer_name = peer_name.to_string();
        tokio::spawn(async move {
            let bus = this.event_bus.clone();
            let publish_failed = |error: String| {
                let bus = bus.clone();
                async move {
                    let _ = bus
                        .publish(CoreEvent::TransferFailed {
                            event: TransferFailedEvent {
                                session_id,
                                direction: RuntimeTransferDirection::Send,
                                error,
                            },
                        })
                        .await;
                }
            };

            let result = client
                .send_request(
                    target_peer,
                    AppRequest::Transfer(TransferRequest::Offer {
                        session_id,
                        files: selected_files.clone(),
                        total_size,
                    }),
                )
                .await;

            match result {
                Ok(AppResponse::Transfer(TransferResponse::OfferResult {
                    accepted: true,
                    key: Some(key),
                    ..
                })) => {
                    info!("Offer accepted for session {}, key received", session_id);
                    if this.discard_cancelled_outbound_offer(session_id, prepared_id) {
                        this.notify_cancel(target_peer, session_id).await;
                        info!(
                            "Offer accepted after local cancel, sent Cancel: session={session_id}"
                        );
                        return;
                    }

                    if let Err(e) = crate::database::ops::create_session(
                        &this.db,
                        session_id,
                        entity::TransferDirection::Send,
                        &peer_id_str,
                        &peer_name,
                        &selected_files,
                        total_size,
                        None,
                        Some(&source_paths),
                    )
                    .await
                    {
                        warn!("发送方创建 DB 记录失败: {}", e);
                        let _ = this
                            .event_bus
                            .publish(CoreEvent::TransferDbError {
                                event: TransferDbErrorEvent {
                                    session_id,
                                    message: format!("保存传输记录失败: {e}"),
                                },
                            })
                            .await;
                    }

                    let send_session = Arc::new(SendSession::new(
                        session_id,
                        target_peer,
                        selected_prepared,
                        &key,
                        this.file_access.clone(),
                        this.event_bus.clone(),
                    ));
                    this.send_sessions.insert(session_id, send_session);
                    this.close_accepted_outbound_offer(session_id, prepared_id);

                    if this.cancelled_outbound_offers.remove(&session_id).is_some() {
                        if let Some((_, session)) = this.send_sessions.remove(&session_id) {
                            session.cancel();
                        }
                        if let Err(e) =
                            crate::database::ops::mark_session_cancelled(&this.db, session_id).await
                        {
                            warn!("DB 标记已撤回发送失败: {}", e);
                        }
                        this.notify_cancel(target_peer, session_id).await;
                        info!(
                            "Send session cancelled immediately after accept: session={session_id}"
                        );
                        return;
                    }

                    let _ = this
                        .event_bus
                        .publish(CoreEvent::TransferAccepted {
                            event: TransferAcceptedEvent { session_id },
                        })
                        .await;
                }
                Ok(AppResponse::Transfer(TransferResponse::OfferResult {
                    accepted: false,
                    reason,
                    ..
                })) => {
                    if this.finish_unaccepted_outbound_offer(session_id, prepared_id) {
                        return;
                    }
                    info!("Offer rejected for session {}: {:?}", session_id, reason);
                    let _ = this
                        .event_bus
                        .publish(CoreEvent::TransferRejected {
                            event: TransferRejectedEvent { session_id, reason },
                        })
                        .await;
                }
                Ok(AppResponse::Transfer(TransferResponse::OfferResult {
                    accepted: true,
                    key: None,
                    ..
                })) => {
                    if this.finish_unaccepted_outbound_offer(session_id, prepared_id) {
                        return;
                    }
                    warn!("Offer accepted 但未收到密钥: session={}", session_id);
                    publish_failed("对方接受但未提供加密密钥".into()).await;
                }
                Ok(other) => {
                    if this.finish_unaccepted_outbound_offer(session_id, prepared_id) {
                        return;
                    }
                    warn!("意外的响应类型: {:?}", other);
                    publish_failed(format!("意外的响应类型: {other:?}")).await;
                }
                Err(e) => {
                    if this.finish_unaccepted_outbound_offer(session_id, prepared_id) {
                        return;
                    }
                    warn!("发送 Offer 失败: {}", e);
                    publish_failed(format!("发送 Offer 失败: {e}")).await;
                }
            }
        });

        Ok(StartSendResult { session_id })
    }

    pub fn get_send_session(&self, session_id: &Uuid) -> Option<Arc<SendSession>> {
        self.send_sessions
            .get(session_id)
            .map(|r| Arc::clone(r.value()))
    }

    pub fn insert_send_session(&self, session_id: Uuid, session: Arc<SendSession>) {
        self.send_sessions.insert(session_id, session);
    }

    pub fn remove_send_session(&self, session_id: &Uuid) {
        self.send_sessions.remove(session_id);
    }

    pub async fn pause_send(&self, session_id: &Uuid) -> AppResult<()> {
        let session = self
            .send_sessions
            .get(session_id)
            .map(|r| Arc::clone(r.value()))
            .ok_or_else(|| AppError::Transfer(format!("发送会话不存在: {session_id}")))?;

        session.cancel();
        let progress = session.get_file_progress();
        crate::database::ops::save_sender_file_progress(&self.db, *session_id, &progress).await?;
        self.coordinator
            .dispatch(
                *session_id,
                crate::transfer::coordinator::CoordinatorInput::User(
                    crate::transfer::coordinator::UserCommand::Pause,
                ),
            )
            .await?;
        crate::database::ops::sync_session_transferred_bytes(&self.db, *session_id).await?;
        self.remove_send_session(session_id);

        if let Err(e) = self
            .client
            .send_request(
                session.peer_id,
                AppRequest::Transfer(TransferRequest::Pause {
                    session_id: *session_id,
                }),
            )
            .await
        {
            warn!("通知对方暂停失败: session={}, {}", session_id, e);
        }

        info!("Send session paused: session={}", session_id);
        Ok(())
    }

    pub async fn cancel_send(&self, session_id: &Uuid) -> AppResult<()> {
        let Some((_, session)) = self.send_sessions.remove(session_id) else {
            if let Some(prepared_id) = self.outbound_offers.get(session_id).map(|o| o.prepared_id) {
                self.cancelled_outbound_offers.insert(*session_id);
                self.prepared.remove(&prepared_id);
                info!("Pending outbound offer cancelled: session={}", session_id);
                return Ok(());
            }

            return Err(AppError::Transfer(format!("发送会话不存在: {session_id}")));
        };

        session.cancel();
        self.notify_cancel(session.peer_id, *session_id).await;

        self.coordinator
            .dispatch(
                *session_id,
                crate::transfer::coordinator::CoordinatorInput::User(
                    crate::transfer::coordinator::UserCommand::Cancel,
                ),
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

    async fn notify_cancel(&self, peer_id: PeerId, session_id: Uuid) {
        if let Err(e) = self
            .client
            .send_request(
                peer_id,
                AppRequest::Transfer(TransferRequest::Cancel {
                    session_id,
                    reason: "用户取消".into(),
                }),
            )
            .await
        {
            warn!("通知对方取消失败: session={}, {}", session_id, e);
        }
    }
}
