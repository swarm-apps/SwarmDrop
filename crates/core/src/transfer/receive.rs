//! 接收方生命周期：缓存入站 Offer / 接受 / 拒绝 / 暂停 / 取消 / receive_session 访问。
//!
//! 这里同时承载 `IncomingTransferRuntime` 的接收侧 helper（`*_impl`），由 manager.rs
//! 中的 trait impl 1-line delegate 调用。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use swarm_p2p_core::libp2p::PeerId;
use tracing::{info, warn};
use uuid::Uuid;

use crate::host::CoreEvent;
use crate::protocol::{
    AppRequest, AppResponse, FileInfo, OfferRejectReason, TransferRequest, TransferResponse,
};
use crate::transfer::crypto::generate_key;
use crate::transfer::incoming::TransferCompleteOutcome;
use crate::transfer::manager::{PendingOffer, TransferManager};
use crate::transfer::progress::{
    RuntimeTransferDirection, TransferDbErrorEvent, TransferFailedEvent,
};
use crate::transfer::receiver::ReceiveSession;
use crate::{AppError, AppResult};

impl TransferManager {
    pub fn cache_inbound_offer(
        &self,
        pending_id: u64,
        peer_id: PeerId,
        peer_name: String,
        session_id: Uuid,
        files: Vec<FileInfo>,
        total_size: u64,
    ) {
        self.pending.insert(
            session_id,
            PendingOffer {
                pending_id,
                peer_id,
                peer_name,
                session_id,
                files,
                total_size,
                created_at: Instant::now(),
            },
        );
    }

    /// 接受传输并启动接收
    pub async fn accept_and_start_receive(
        &self,
        session_id: &Uuid,
        save_location: crate::host::CoreSaveLocation,
    ) -> AppResult<()> {
        let (_, offer) = self
            .pending
            .remove(session_id)
            .ok_or_else(|| AppError::Transfer(format!("pending offer not found: {session_id}")))?;

        let key = generate_key();
        info!("Accepting transfer offer: session={}", session_id);

        let response = AppResponse::Transfer(TransferResponse::OfferResult {
            accepted: true,
            key: Some(key),
            reason: None,
        });
        self.client
            .send_response(offer.pending_id, response)
            .await
            .map_err(|e| AppError::Transfer(format!("回复 OfferResult 失败: {e}")))?;

        let peer_id_str = offer.peer_id.to_string();
        if let Err(e) = crate::database::ops::create_session(
            &self.db,
            offer.session_id,
            entity::TransferDirection::Receive,
            &peer_id_str,
            &offer.peer_name,
            &offer.files,
            offer.total_size,
            Some(save_location.clone()),
            None,
        )
        .await
        {
            warn!("接收方创建 DB 记录失败: {}", e);
            let _ = self
                .event_bus
                .publish(CoreEvent::TransferDbError {
                    event: TransferDbErrorEvent {
                        session_id: offer.session_id,
                        message: format!("保存传输记录失败: {e}"),
                    },
                })
                .await;
        }

        self.start_receive_session(
            offer.session_id,
            offer.peer_id,
            offer.files,
            offer.total_size,
            save_location,
            &key,
            HashMap::new(),
        );

        Ok(())
    }

    pub async fn reject_and_respond(&self, session_id: &Uuid) -> AppResult<()> {
        let (_, offer) = self
            .pending
            .remove(session_id)
            .ok_or_else(|| AppError::Transfer(format!("pending offer not found: {session_id}")))?;

        info!("Rejecting transfer offer: session={}", session_id);

        let response = AppResponse::Transfer(TransferResponse::OfferResult {
            accepted: false,
            key: None,
            reason: Some(OfferRejectReason::UserDeclined),
        });
        self.client
            .send_response(offer.pending_id, response)
            .await
            .map_err(|e| AppError::Transfer(format!("回复拒绝 OfferResult 失败: {e}")))
    }

    pub async fn pause_receive(&self, session_id: &Uuid) -> AppResult<()> {
        let session = self
            .receive_sessions
            .get(session_id)
            .map(|r| Arc::clone(r.value()))
            .ok_or_else(|| AppError::Transfer(format!("接收会话不存在: {session_id}")))?;

        session.cancel_and_wait().await;
        self.coordinator
            .dispatch(
                *session_id,
                crate::transfer::coordinator::CoordinatorInput::User(
                    crate::transfer::coordinator::UserCommand::Pause,
                ),
            )
            .await?;
        crate::database::ops::sync_session_transferred_bytes(&self.db, *session_id).await?;
        self.receive_sessions.remove(session_id);

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

        info!("Receive session paused: session={}", session_id);
        Ok(())
    }

    pub async fn cancel_receive(&self, session_id: &Uuid) -> AppResult<()> {
        let session = self
            .receive_sessions
            .get(session_id)
            .map(|r| Arc::clone(r.value()))
            .ok_or_else(|| AppError::Transfer(format!("接收会话不存在: {session_id}")))?;

        session.cancel_and_wait().await;
        session.send_cancel().await;
        session.cleanup_part_files().await;
        self.receive_sessions.remove(session_id);
        // 状态决策经 Coordinator：写 phase+status(桥接)+finished_at 并发 projection。
        self.coordinator
            .dispatch(
                *session_id,
                crate::transfer::coordinator::CoordinatorInput::User(
                    crate::transfer::coordinator::UserCommand::Cancel,
                ),
            )
            .await?;
        info!("Receive session cancelled: session={}", session_id);
        Ok(())
    }

    pub fn get_receive_session(&self, session_id: &Uuid) -> Option<Arc<ReceiveSession>> {
        self.receive_sessions
            .get(session_id)
            .map(|r| Arc::clone(r.value()))
    }

    pub fn remove_receive_session(&self, session_id: &Uuid) {
        self.receive_sessions.remove(session_id);
    }

    /// 公开接口：创建 ReceiveSession 并开始拉取
    #[expect(
        clippy::too_many_arguments,
        reason = "传输会话初始化必须接收完整上下文（session_id / peer / files / 元信息 / 加密密钥 / 续传位图），无更小的有意义子集"
    )]
    pub fn start_receive_from_offer(
        &self,
        session_id: Uuid,
        peer_id: PeerId,
        files: Vec<FileInfo>,
        total_size: u64,
        save_location: crate::host::CoreSaveLocation,
        key: &[u8; 32],
        initial_bitmaps: HashMap<u32, Vec<u8>>,
    ) {
        self.start_receive_session(
            session_id,
            peer_id,
            files,
            total_size,
            save_location,
            key,
            initial_bitmaps,
        );
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "同 start_receive_from_offer：传输会话初始化必须接收完整上下文"
    )]
    pub(super) fn start_receive_session(
        &self,
        session_id: Uuid,
        peer_id: PeerId,
        files: Vec<FileInfo>,
        total_size: u64,
        save_location: crate::host::CoreSaveLocation,
        key: &[u8; 32],
        initial_bitmaps: HashMap<u32, Vec<u8>>,
    ) {
        let receive_session = Arc::new(ReceiveSession::new(
            session_id,
            peer_id,
            files,
            total_size,
            self.file_access.clone(),
            self.event_bus.clone(),
            self.db.clone(),
            save_location,
            key,
            self.client.clone(),
            initial_bitmaps,
        ));
        self.receive_sessions
            .insert(session_id, receive_session.clone());
        let sessions_map = self.receive_sessions.clone();
        receive_session.start_pulling(move |sid| {
            sessions_map.remove(sid);
        });
    }
}

// ============ IncomingTransferRuntime 接收侧 helper（被 manager.rs 中 trait impl 调用） ============

impl TransferManager {
    pub(super) async fn handle_chunk_request_impl(
        &self,
        session_id: Uuid,
        file_id: u32,
        chunk_index: u32,
    ) -> AppResult<TransferResponse> {
        match self.get_send_session(&session_id) {
            Some(session) => session.handle_chunk_request(file_id, chunk_index).await,
            None => Ok(TransferResponse::ChunkError {
                session_id,
                file_id,
                chunk_index,
                error: "发送会话不存在".into(),
            }),
        }
    }

    pub(super) async fn handle_complete_impl(
        &self,
        session_id: Uuid,
    ) -> AppResult<TransferCompleteOutcome> {
        let (total_bytes, elapsed_ms) = self
            .get_send_session(&session_id)
            .map(|session| {
                session.handle_complete();
                (session.total_bytes_sent(), session.elapsed_ms())
            })
            .unwrap_or((0, 0));
        self.remove_send_session(&session_id);

        let mut db_error = None;
        if let Err(e) = crate::database::ops::mark_session_completed(&self.db, session_id).await {
            warn!("DB 标记发送完成失败: {}", e);
            db_error = Some(TransferDbErrorEvent {
                session_id,
                message: format!("保存完成状态失败: {e}"),
            });
        } else {
            // mark 已双写 phase=terminal/completed；额外发 projection（reduce 对 terminal 返回 None，故直发）。
            let _ = self.coordinator.publish_projection(session_id).await;
        }

        Ok(TransferCompleteOutcome {
            event: crate::transfer::progress::TransferCompleteEvent {
                session_id,
                direction: RuntimeTransferDirection::Send,
                total_bytes,
                elapsed_ms,
                save_location: None,
            },
            db_error,
        })
    }

    pub(super) async fn handle_cancel_impl(
        &self,
        session_id: Uuid,
        reason: String,
    ) -> AppResult<TransferFailedEvent> {
        if let Some(session) = self.get_send_session(&session_id) {
            session.handle_cancel();
            self.remove_send_session(&session_id);
        }
        if let Some(session) = self.get_receive_session(&session_id) {
            self.remove_receive_session(&session_id);
            tokio::spawn(async move {
                session.cancel_and_wait().await;
                session.cleanup_part_files().await;
            });
        }
        if let Err(e) = crate::database::ops::mark_session_cancelled(&self.db, session_id).await {
            warn!("DB 标记取消失败: {}", e);
        }
        Ok(TransferFailedEvent {
            session_id,
            direction: RuntimeTransferDirection::Unknown,
            error: format!("对方取消: {reason}"),
        })
    }

    pub(super) async fn handle_pause_impl(
        &self,
        session_id: Uuid,
    ) -> AppResult<crate::transfer::progress::TransferPausedEvent> {
        let direction = if let Some(session) = self.get_send_session(&session_id) {
            let progress = session.get_file_progress();
            let _ =
                crate::database::ops::save_sender_file_progress(&self.db, session_id, &progress)
                    .await;
            session.cancel();
            self.remove_send_session(&session_id);
            RuntimeTransferDirection::Send
        } else if let Some(session) = self.get_receive_session(&session_id) {
            self.remove_receive_session(&session_id);
            session.cancel_and_wait().await;
            RuntimeTransferDirection::Receive
        } else {
            RuntimeTransferDirection::Unknown
        };

        if let Err(e) = crate::database::ops::pause_session(&self.db, session_id).await {
            warn!("DB 标记暂停失败: {}", e);
        }

        Ok(crate::transfer::progress::TransferPausedEvent {
            session_id,
            direction,
        })
    }
}
