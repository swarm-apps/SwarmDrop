//! 接收方生命周期：缓存入站 Offer / 接受 / 拒绝 / 暂停 / 取消 / receive_actor 访问。
//!
//! 这里同时承载 `IncomingTransferRuntime` 的接收侧 helper（`*_impl`），由 manager.rs
//! 中的 trait impl 1-line delegate 调用。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use swarmdrop_net::NodeId;
use tokio::sync::oneshot;
use tracing::{info, warn};
use uuid::Uuid;

use crate::actor::receiver::ReceiverActor;
use crate::coordinator::{CoordinatorInput, TransferState, UserCommand};
use crate::manager::{PendingOffer, TransferManager};
use crate::policy::ReceivePolicyDecision;
use crate::progress::{RuntimeTransferDirection, TransferFailedEvent};
use crate::protocol::{FileInfo, OfferRejectReason, TransferOrigin, TransferResponse};
use crate::store::CreateSessionInput;
use crate::{AppError, AppResult};

impl TransferManager {
    /// 落库一条 `offered` 入站接收会话，并把策略快照随建会话一次写入。
    /// `cache_inbound_offer`（待用户决定）与 `record_rejected_inbound_offer`（策略直拒）共用。
    #[expect(
        clippy::too_many_arguments,
        reason = "建入站会话需完整对端/会话/策略上下文，无更小的有意义子集"
    )]
    async fn create_offered_inbound_session(
        &self,
        peer_id: &NodeId,
        peer_name: &str,
        session_id: Uuid,
        files: &[FileInfo],
        total_size: u64,
        origin: TransferOrigin,
        policy_decision: &ReceivePolicyDecision,
    ) -> AppResult<()> {
        let peer_id_str = peer_id.to_string();
        self.store
            .create_session(CreateSessionInput {
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
                origin: Some(origin),
            })
            .await
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "缓存入站 offer 需要完整的对端与会话上下文"
    )]
    pub async fn cache_inbound_offer(
        &self,
        peer_id: NodeId,
        peer_name: String,
        session_id: Uuid,
        files: Vec<FileInfo>,
        total_size: u64,
        origin: TransferOrigin,
        policy_decision: ReceivePolicyDecision,
    ) -> AppResult<oneshot::Receiver<TransferResponse>> {
        self.create_offered_inbound_session(
            &peer_id,
            &peer_name,
            session_id,
            &files,
            total_size,
            origin,
            &policy_decision,
        )
        .await?;
        self.coordinator.publish_projection(session_id).await?;

        // responder：transfer-ctrl handler await 它拿到用户/自动决策。
        let (responder, rx) = oneshot::channel();
        self.pending.insert(
            session_id,
            PendingOffer {
                peer_id,
                peer_name,
                session_id,
                files,
                total_size,
                created_at: Instant::now(),
                responder,
            },
        );

        Ok(rx)
    }

    /// 记录被策略拒绝的入站 Offer。该记录只进入活动与恢复，不会进入收件箱。
    #[expect(
        clippy::too_many_arguments,
        reason = "记录被拒 offer 需完整对端/会话/策略上下文"
    )]
    pub async fn record_rejected_inbound_offer(
        &self,
        peer_id: NodeId,
        peer_name: String,
        session_id: Uuid,
        files: Vec<FileInfo>,
        total_size: u64,
        origin: TransferOrigin,
        policy_decision: ReceivePolicyDecision,
    ) -> AppResult<()> {
        self.create_offered_inbound_session(
            &peer_id,
            &peer_name,
            session_id,
            &files,
            total_size,
            origin,
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

    /// Peek 挂起入站 offer 的来源 `PeerId`（不移除），供 MCP 代收门控校验用。
    ///
    /// 返回 `None` 表示该 session 没有挂起 offer——已被接受/拒绝，或已过挂起窗口被回收。
    pub fn pending_offer_peer(&self, session_id: &Uuid) -> Option<NodeId> {
        self.pending.get(session_id).map(|offer| offer.peer_id)
    }

    /// 接受传输并启动接收
    ///
    /// **安全序**：先注册 ReceiverActor，再解决应答通道——对端 sender 收到
    /// `accepted:true` 后立即打开数据面流，接收 actor 必须已就绪，否则 Hello 被拒。
    pub async fn accept_and_start_receive(
        &self,
        session_id: &Uuid,
        save_location: crate::host::CoreSaveLocation,
    ) -> AppResult<()> {
        let (_, offer) = self
            .pending
            .remove(session_id)
            .ok_or_else(|| AppError::Transfer(format!("pending offer not found: {session_id}")))?;

        info!("Accepting transfer offer: session={}", session_id);

        self.store
            .update_session_save_path(offer.session_id, save_location.clone())
            .await?;

        self.start_receive_actor(
            0,
            offer.session_id,
            offer.peer_id,
            offer.files,
            offer.total_size,
            save_location,
            HashMap::new(),
        );

        // 解决 transfer-ctrl handler 的应答通道 → 对端得 accepted:true，开始推送
        let _ = offer.responder.send(TransferResponse::OfferResult {
            accepted: true,
            reason: None,
        });

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

        let _ = offer.responder.send(TransferResponse::OfferResult {
            accepted: false,
            reason: Some(OfferRejectReason::UserDeclined),
        });
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
            .get_receive_actor(session_id)
            .ok_or_else(|| AppError::Transfer(format!("接收会话不存在: {session_id}")))?;

        session.cancel_and_wait().await;
        // projection 的 transferredBytes 直接 SUM 文件级（文件进度已增量落库），
        // 无需在 dispatch 前手工 sync session 级。
        self.coordinator
            .dispatch(
                *session_id,
                crate::coordinator::CoordinatorInput::User(crate::coordinator::UserCommand::Pause),
            )
            .await?;
        self.remove_receive_actor(session_id);
        self.notify_pause(session.peer_id, *session_id).await;

        info!("Receive session paused: session={}", session_id);
        Ok(())
    }

    pub async fn cancel_receive(&self, session_id: &Uuid) -> AppResult<()> {
        let session = self
            .get_receive_actor(session_id)
            .ok_or_else(|| AppError::Transfer(format!("接收会话不存在: {session_id}")))?;

        session.cancel_and_wait().await;
        // Cancel 通知上提到 manager 层，与发送侧对称（ReceiverActor 不再持 endpoint）
        self.notify_cancel(session.peer_id, *session_id).await;
        session.cleanup_part_files().await;
        self.remove_receive_actor(session_id);
        // 状态决策经 Coordinator：写 phase+status(桥接)+finished_at 并发 projection。
        self.coordinator
            .dispatch(
                *session_id,
                crate::coordinator::CoordinatorInput::User(crate::coordinator::UserCommand::Cancel),
            )
            .await?;
        info!("Receive session cancelled: session={}", session_id);
        Ok(())
    }

    pub fn get_receive_actor(&self, session_id: &Uuid) -> Option<Arc<ReceiverActor>> {
        self.actors.get_receive(session_id)
    }

    pub fn remove_receive_actor(&self, session_id: &Uuid) -> Option<Arc<ReceiverActor>> {
        self.actors.remove_receive(session_id)
    }

    /// 创建 ReceiverActor 并注册到 ActorRegistry（接受 Offer / 恢复重建共用）。
    #[expect(
        clippy::too_many_arguments,
        reason = "传输会话初始化必须接收完整上下文（session_id / peer / files / 元信息 / 续传位图），无更小的有意义子集"
    )]
    pub(crate) fn start_receive_actor(
        &self,
        epoch: i64,
        session_id: Uuid,
        peer_id: NodeId,
        files: Vec<FileInfo>,
        total_size: u64,
        save_location: crate::host::CoreSaveLocation,
        initial_bitmaps: HashMap<u32, Vec<u8>>,
    ) {
        let receive_actor = Arc::new(ReceiverActor::new(
            session_id,
            peer_id,
            files,
            total_size,
            self.file_access.clone(),
            self.events.clone(),
            self.store.clone(),
            self.coordinator.clone(),
            save_location,
            initial_bitmaps,
        ));
        self.actors
            .insert_receive(session_id, epoch, receive_actor.clone());
    }
}

// ============ IncomingTransferRuntime 接收侧 helper（被 manager.rs 中 trait impl 调用） ============

impl TransferManager {
    pub(crate) async fn handle_cancel_impl(
        &self,
        session_id: Uuid,
        reason: String,
    ) -> AppResult<TransferFailedEvent> {
        if let Some(session) = self.get_send_actor(&session_id) {
            session.handle_cancel();
            self.remove_send_actor(&session_id);
        }
        if let Some(session) = self.get_receive_actor(&session_id) {
            self.remove_receive_actor(&session_id);
            n0_future::task::spawn(async move {
                session.cancel_and_wait().await;
                session.cleanup_part_files().await;
            });
        }
        // 对端取消 → 状态机 Network{RemoteCancelled}（写 terminal/cancelled + 发 projection）。
        if let Err(e) = self
            .coordinator
            .dispatch_network_current(
                session_id,
                crate::coordinator::NetworkSignal::RemoteCancelled,
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
    pub(crate) async fn handle_peer_disconnected_impl(&self, peer_id: NodeId) {
        let peer_str = peer_id.to_string();
        let ids = match self.store.find_active_session_ids_by_peer(&peer_str).await {
            Ok(ids) => ids,
            Err(e) => {
                warn!("查询 peer {} 的 active 会话失败: {}", peer_str, e);
                return;
            }
        };
        for session_id in ids {
            if let Some(session) = self.remove_send_actor(&session_id) {
                session.cancel();
            }
            if let Some(session) = self.get_receive_actor(&session_id) {
                self.remove_receive_actor(&session_id);
                session.cancel_and_wait().await;
            }
            if let Err(e) = self
                .coordinator
                .dispatch_network_current(
                    session_id,
                    crate::coordinator::NetworkSignal::Interrupted,
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
    ) -> AppResult<crate::progress::TransferPausedEvent> {
        let direction = if let Some(session) = self.get_send_actor(&session_id) {
            let progress = session.get_file_progress();
            let _ = self
                .store
                .save_sender_file_progress(session_id, &progress)
                .await;
            session.cancel();
            self.remove_send_actor(&session_id);
            RuntimeTransferDirection::Send
        } else if let Some(session) = self.get_receive_actor(&session_id) {
            self.remove_receive_actor(&session_id);
            session.cancel_and_wait().await;
            RuntimeTransferDirection::Receive
        } else {
            RuntimeTransferDirection::Unknown
        };

        // 对端暂停 → 状态机 Network{RemotePaused}（写 suspended/RemotePaused + 发 projection），
        // 与本地 pause 的 LocalPaused 区分开——这正是 3.3 要落实的本地/对端 reason 区分。
        if let Err(e) = self
            .coordinator
            .dispatch_network_current(session_id, crate::coordinator::NetworkSignal::RemotePaused)
            .await
        {
            warn!("dispatch 对端暂停失败: {}", e);
        }

        Ok(crate::progress::TransferPausedEvent {
            session_id,
            direction,
        })
    }
}
