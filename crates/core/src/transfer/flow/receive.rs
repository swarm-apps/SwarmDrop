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

use crate::database::ops::CreateSessionInput;
use crate::protocol::{
    AppRequest, AppResponse, FileInfo, OfferRejectReason, TransferRequest, TransferResponse,
};
use crate::transfer::coordinator::{CoordinatorInput, TransferState, UserCommand};
use crate::transfer::wire::crypto::generate_key;
use crate::transfer::manager::{PendingOffer, TransferManager};
use crate::transfer::policy::ReceivePolicyDecision;
use crate::transfer::progress::{RuntimeTransferDirection, TransferFailedEvent};
use crate::transfer::actor::receiver::ReceiveSession;
use crate::{AppError, AppResult};

impl TransferManager {
    /// 落库一条 `offered` 入站接收会话，并把策略快照随建会话一次写入。
    /// `cache_inbound_offer`（待用户决定）与 `record_rejected_inbound_offer`（策略直拒）共用。
    async fn create_offered_inbound_session(
        &self,
        peer_id: &PeerId,
        peer_name: &str,
        session_id: Uuid,
        files: &[FileInfo],
        total_size: u64,
        policy_decision: &ReceivePolicyDecision,
    ) -> AppResult<()> {
        let peer_id_str = peer_id.to_string();
        crate::database::ops::create_session(
            &self.db,
            CreateSessionInput {
                session_id,
                direction: entity::TransferDirection::Receive,
                peer_id: &peer_id_str,
                peer_name,
                files,
                total_size,
                save_path: None,
                source_paths: None,
                lifecycle: TransferState::offered(0),
                policy: Some((policy_decision.action_name(), &policy_decision.reason)),
            },
        )
        .await
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "缓存入站 offer 需要完整的对端与会话上下文"
    )]
    pub async fn cache_inbound_offer(
        &self,
        pending_id: u64,
        peer_id: PeerId,
        peer_name: String,
        session_id: Uuid,
        files: Vec<FileInfo>,
        total_size: u64,
        policy_decision: ReceivePolicyDecision,
    ) -> AppResult<()> {
        self.create_offered_inbound_session(
            &peer_id,
            &peer_name,
            session_id,
            &files,
            total_size,
            &policy_decision,
        )
        .await?;
        self.coordinator.publish_projection(session_id).await?;

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

        Ok(())
    }

    /// 记录被策略拒绝的入站 Offer。该记录只进入活动与恢复，不会进入收件箱。
    pub async fn record_rejected_inbound_offer(
        &self,
        peer_id: PeerId,
        peer_name: String,
        session_id: Uuid,
        files: Vec<FileInfo>,
        total_size: u64,
        policy_decision: ReceivePolicyDecision,
    ) -> AppResult<()> {
        self.create_offered_inbound_session(
            &peer_id,
            &peer_name,
            session_id,
            &files,
            total_size,
            &policy_decision,
        )
        .await?;
        // 终态经状态机：offered → terminal/rejected（policy reason 已随建会话写入
        // policy_reason，前端据此展示）。
        self.coordinator
            .dispatch(session_id, CoordinatorInput::User(UserCommand::Reject))
            .await?;

        Ok(())
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

        crate::database::ops::update_session_save_path(
            &self.db,
            offer.session_id,
            save_location.clone(),
        )
        .await?;

        self.start_receive_session(
            0,
            offer.session_id,
            offer.peer_id,
            offer.files,
            offer.total_size,
            save_location,
            &key,
            HashMap::new(),
        );

        self.coordinator
            .dispatch(
                offer.session_id,
                CoordinatorInput::User(UserCommand::Accept),
            )
            .await?;

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
            .map_err(|e| AppError::Transfer(format!("回复拒绝 OfferResult 失败: {e}")))?;
        self.coordinator
            .dispatch(
                offer.session_id,
                CoordinatorInput::User(UserCommand::Reject),
            )
            .await?;
        Ok(())
    }

    pub async fn pause_receive(&self, session_id: &Uuid) -> AppResult<()> {
        let session = self
            .get_receive_session(session_id)
            .ok_or_else(|| AppError::Transfer(format!("接收会话不存在: {session_id}")))?;

        session.cancel_and_wait().await;
        // 先把文件级进度汇总到 session 级，再 dispatch：dispatch 当帧 emit 的 projection
        // 才带正确的 transferredBytes，避免挂起态在前端显示 0%。
        crate::database::ops::sync_session_transferred_bytes(&self.db, *session_id).await?;
        self.coordinator
            .dispatch(
                *session_id,
                crate::transfer::coordinator::CoordinatorInput::User(
                    crate::transfer::coordinator::UserCommand::Pause,
                ),
            )
            .await?;
        self.remove_receive_session(session_id);

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
            .get_receive_session(session_id)
            .ok_or_else(|| AppError::Transfer(format!("接收会话不存在: {session_id}")))?;

        session.cancel_and_wait().await;
        session.send_cancel().await;
        session.cleanup_part_files().await;
        self.remove_receive_session(session_id);
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
        self.actors.get_receive(session_id)
    }

    pub fn remove_receive_session(&self, session_id: &Uuid) -> Option<Arc<ReceiveSession>> {
        self.actors.remove_receive(session_id)
    }

    /// 创建 ReceiveSession 并注册到 ActorRegistry（接受 Offer / 恢复重建共用）。
    #[expect(
        clippy::too_many_arguments,
        reason = "传输会话初始化必须接收完整上下文（session_id / peer / files / 元信息 / 加密密钥 / 续传位图），无更小的有意义子集"
    )]
    pub(crate) fn start_receive_session(
        &self,
        epoch: i64,
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
            self.coordinator.clone(),
            save_location,
            key,
            self.client.clone(),
            initial_bitmaps,
        ));
        self.actors
            .insert_receive(session_id, epoch, receive_session.clone());
    }
}

// ============ IncomingTransferRuntime 接收侧 helper（被 manager.rs 中 trait impl 调用） ============

impl TransferManager {
    pub(crate) async fn handle_cancel_impl(
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
        // 对端取消 → 状态机 Network{RemoteCancelled}（写 terminal/cancelled + 发 projection）。
        if let Err(e) = self
            .coordinator
            .dispatch_network_current(
                session_id,
                crate::transfer::coordinator::NetworkSignal::RemoteCancelled,
            )
            .await
        {
            warn!("dispatch 对端取消失败: {}", e);
        }
        Ok(TransferFailedEvent {
            session_id,
            direction: RuntimeTransferDirection::Unknown,
            error: format!("对方取消: {reason}"),
        })
    }

    /// 对端断连：把该 peer 当前所有 active 传输转为 recoverable suspended(Interrupted)。
    ///
    /// 先取消内存中的 send/receive 会话（cancel 优先于 error，run_data_channel 返回 Ok(false) 不 fail），
    /// 再经状态机 `Network{Interrupted}` 写 suspended/Interrupted/recoverable + 发 projection。
    /// 发送端会话由 data-channel 推送驱动、自身不轮询，靠此 hook 才能感知断连。
    pub(crate) async fn handle_peer_disconnected_impl(&self, peer_id: PeerId) {
        let peer_str = peer_id.to_string();
        let ids = match crate::database::ops::find_active_session_ids_by_peer(&self.db, &peer_str)
            .await
        {
            Ok(ids) => ids,
            Err(e) => {
                warn!("查询 peer {} 的 active 会话失败: {}", peer_str, e);
                return;
            }
        };
        for session_id in ids {
            if let Some(session) = self.remove_send_session(&session_id) {
                session.cancel();
            }
            if let Some(session) = self.get_receive_session(&session_id) {
                self.remove_receive_session(&session_id);
                session.cancel_and_wait().await;
            }
            // 先汇总 session 级 transferredBytes，使中断帧 projection 进度正确（否则显示 0%）。
            if let Err(e) =
                crate::database::ops::sync_session_transferred_bytes(&self.db, session_id).await
            {
                warn!(
                    "sync session transferred_bytes 失败: session={}, {}",
                    session_id, e
                );
            }
            if let Err(e) = self
                .coordinator
                .dispatch_network_current(
                    session_id,
                    crate::transfer::coordinator::NetworkSignal::Interrupted,
                )
                .await
            {
                warn!("dispatch 对端断连中断失败: session={}, {}", session_id, e);
            }
        }
    }

    pub(crate) async fn handle_pause_impl(
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

        // 对端暂停 → 状态机 Network{RemotePaused}（写 suspended/RemotePaused + 发 projection），
        // 与本地 pause 的 LocalPaused 区分开——这正是 3.3 要落实的本地/对端 reason 区分。
        // 先汇总 session 级 transferredBytes，否则 dispatch emit 的挂起 projection 进度为 0。
        if let Err(e) =
            crate::database::ops::sync_session_transferred_bytes(&self.db, session_id).await
        {
            warn!(
                "sync session transferred_bytes 失败: session={}, {}",
                session_id, e
            );
        }
        if let Err(e) = self
            .coordinator
            .dispatch_network_current(
                session_id,
                crate::transfer::coordinator::NetworkSignal::RemotePaused,
            )
            .await
        {
            warn!("dispatch 对端暂停失败: {}", e);
        }

        Ok(crate::transfer::progress::TransferPausedEvent {
            session_id,
            direction,
        })
    }
}
