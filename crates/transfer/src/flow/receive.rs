//! 接收方生命周期：缓存入站 Offer / 接受 / 拒绝 / 暂停 / 取消 / receive_actor 访问。
//!
//! 这里同时承载 `IncomingTransferRuntime` 的接收侧 helper（`*_impl`），由 manager.rs
//! 中的 trait impl 1-line delegate 调用。

use n0_future::time::Instant;
use std::collections::HashMap;
use std::sync::Arc;

use swarmdrop_net::NodeId;
use tokio::sync::oneshot;
use tracing::{info, warn};
use uuid::Uuid;

use crate::actor::receiver::ReceiverActor;
use crate::coordinator::{CoordinatorInput, TransferState, UserCommand};
use crate::manager::{PendingOffer, PendingOfferSummary, TransferManager};
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

    /// 当前挂起（待用户决定）入站 offer 的只读快照。
    ///
    /// 桌面/移动经 `transfer-offer` 事件驱动 UI；Web 壳需要 pull 型查询当前 pending
    /// （事件流之外的补查），故提供此只读访问器——不改任何行为。
    pub fn pending_offers(&self) -> Vec<PendingOfferSummary> {
        self.pending
            .iter()
            .map(|entry| {
                let offer = entry.value();
                PendingOfferSummary {
                    session_id: offer.session_id,
                    peer_id: offer.peer_id,
                    peer_name: offer.peer_name.clone(),
                    files: offer.files.clone(),
                    total_size: offer.total_size,
                }
            })
            .collect()
    }

    /// 接受传输并启动接收。
    ///
    /// ## 越线点（point of no return）
    ///
    /// `responder.send(accepted: true)` 是本函数的**越线点**：过了它对端立刻开始推数据，
    /// 本机无法单方面撤回。三步布局因此是刻意的，不要随手调换：
    ///
    /// 1. **越线前**的可失败步骤全部收在 [`Self::prepare_accept`]。任何一步失败都把
    ///    `offer` **放回 `pending`** 再返回 `Err` —— 否则那条 offer 从 UI 上消失、
    ///    `responder` 随之 drop（对端 RPC 直接断），用户连重试的入口都没有。
    /// 2. `start_receive_actor` 不可失败，且**必须早于**应答：对端收到 `accepted:true`
    ///    后立即打开数据面流，actor 没就绪的话 Hello 会被拒。
    /// 3. **越线后什么都不做** —— 这是本函数满足越线规则的方式，也是最强的那种：
    ///    没有可失败的步骤，就没有「已经发生却报失败」的可能。
    ///    ⚠️ 往这个函数尾部加任何 `?` 之前先想清楚：它失败时对端已经在推数据了。
    ///
    /// 原本 `dispatch(Accept)` 是写在应答**之后**并带 `?` 的 —— 它一失败，用户看到
    /// 「接收失败」而文件正在往硬盘里写。挪到越线前之后这条路径不复存在。
    /// 同规则的其他做法见 `handle_cancel_impl` / `handle_pause_impl` /
    /// `handle_peer_disconnected_impl`：那几处确实挪不动，于是一律 `if let Err(e) = … { warn!(…) }`。
    pub async fn accept_and_start_receive(
        &self,
        session_id: &Uuid,
        save_location: crate::host::CoreSaveLocation,
    ) -> AppResult<()> {
        let (_, offer) = self.pending.remove(session_id).ok_or_else(|| {
            AppError::SessionNotFound(format!("pending offer not found: {session_id}"))
        })?;

        info!("Accepting transfer offer: session={}", session_id);

        if let Err(e) = self.prepare_accept(&offer, &save_location).await {
            // 放回待决表：越线还没发生，这次接受可以整个重来。
            self.pending.insert(*session_id, offer);
            return Err(e);
        }

        self.start_receive_actor(
            0,
            offer.session_id,
            offer.peer_id,
            offer.files,
            offer.total_size,
            save_location,
            HashMap::new(),
        );

        // 解决 transfer-ctrl handler 的应答通道 → 对端得 accepted:true，开始推送。
        if offer
            .responder
            .send(TransferResponse::OfferResult {
                accepted: true,
                reason: None,
            })
            .is_err()
        {
            // 应答通道已关闭 = 对端**没有**收到接受（RPC 早就超时了），越线并未发生。
            // 这是唯一需要主动补偿的分支：会话已经是 active、actor 已注册，得推回终态，
            // 否则它会一直挂在活动列表里等一份永远不会来的数据。
            warn!(
                "接受已就绪但应答通道已关闭（对端 RPC 已断），回滚会话: session={}",
                session_id
            );
            if let Some(actor) = self.remove_receive_actor(session_id) {
                actor.cancel_and_wait().await;
            }
            if let Err(e) = self
                .coordinator
                .dispatch(*session_id, CoordinatorInput::User(UserCommand::Cancel))
                .await
            {
                warn!("回滚接受时写取消状态失败: session={}, {}", session_id, e);
            }
            return Err(AppError::Transfer(format!(
                "对端已断开，接受未送达: {session_id}"
            )));
        }

        Ok(())
    }

    /// 越线前的可失败步骤。失败时调用方负责把 `offer` 放回 `pending`
    /// （见 [`Self::accept_and_start_receive`] 的越线点说明）。
    ///
    /// **`dispatch(Accept)` 属于这里，不属于应答之后。** 它只写本机 DB，完全可以先做；
    /// 先做的话失败还能干净地退出（offer 放回、用户重试），做在后面就只剩下两条烂路：
    /// 报 `Err`（用户以为没收上，实际正在收）或者吞掉（活动列表永远停在 offered）。
    async fn prepare_accept(
        &self,
        offer: &PendingOffer,
        save_location: &crate::host::CoreSaveLocation,
    ) -> AppResult<()> {
        self.store
            .update_session_save_path(offer.session_id, save_location.clone())
            .await?;
        self.coordinator
            .dispatch(
                offer.session_id,
                CoordinatorInput::User(UserCommand::Accept),
            )
            .await?;
        Ok(())
    }

    /// 拒绝入站 offer。
    ///
    /// 与 [`accept_and_start_receive`](Self::accept_and_start_receive) 同一条越线规则、
    /// 同一种满足方式：状态转换写在应答**之前**，失败就把 offer 放回待决表让用户重试。
    /// 写在之后的话它一失败，用户看到「拒绝失败」而对端已经按拒绝收尾了 —— 再点一次
    /// 只会得到「offer 不存在」。
    pub async fn reject_and_respond(&self, session_id: &Uuid) -> AppResult<()> {
        let (_, offer) = self.pending.remove(session_id).ok_or_else(|| {
            AppError::SessionNotFound(format!("pending offer not found: {session_id}"))
        })?;

        info!("Rejecting transfer offer: session={}", session_id);

        if let Err(e) = self
            .coordinator
            .dispatch(
                offer.session_id,
                CoordinatorInput::User(UserCommand::Reject),
            )
            .await
        {
            self.pending.insert(*session_id, offer);
            return Err(e);
        }

        // ==== 越线：此后无可失败步骤 ====
        // 应答通道已关闭无需补偿：对端 RPC 早就断了，而拒绝本就是终态，本机记账已完成。
        let _ = offer.responder.send(TransferResponse::OfferResult {
            accepted: false,
            reason: Some(OfferRejectReason::UserDeclined),
        });
        Ok(())
    }

    /// 暂停一条接收会话。
    ///
    /// 顺序与 [`pause_send`](Self::pause_send) 一致，**`notify_pause` 必须早于关闭 actor**
    /// ——完整推导写在那里，一句话是：关流不携带原因，对端只会当成 `Interrupted`，而那条
    /// 守卫先满足之后 `RemotePaused` 就再也进不来了。
    ///
    /// 接收方向不必落进度：文件进度由 `persist_chunk` 增量落库，projection 的
    /// transferredBytes 直接 SUM 文件级，本来就是准的。
    pub async fn pause_receive(&self, session_id: &Uuid) -> AppResult<()> {
        let session = self
            .get_receive_actor(session_id)
            .ok_or_else(|| AppError::SessionNotFound(format!("接收会话不存在: {session_id}")))?;

        self.coordinator
            .dispatch(
                *session_id,
                crate::coordinator::CoordinatorInput::User(crate::coordinator::UserCommand::Pause),
            )
            .await?;
        self.notify_pause(session.peer_id, *session_id).await;
        session.cancel_and_wait().await;
        self.remove_receive_actor(session_id);

        info!("Receive session paused: session={}", session_id);
        Ok(())
    }

    pub async fn cancel_receive(&self, session_id: &Uuid) -> AppResult<()> {
        let session = self
            .get_receive_actor(session_id)
            .ok_or_else(|| AppError::SessionNotFound(format!("接收会话不存在: {session_id}")))?;

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
