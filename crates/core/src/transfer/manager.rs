//! 传输管理器
//!
//! 管理 Offer 协议（发送、接受、拒绝、暂停、恢复）和活跃传输会话。
//! 文件 I/O / 事件推送 / 数据库读写全部通过 trait 抽象，host (Tauri / RN) 各自实现。
//!
//! 本文件保留 `TransferManager` 的结构体定义、构造、清理任务、访问器，以及两个
//! 顶层 trait impl（`TransferRuntime` / `IncomingTransferRuntime`）。具体业务方法
//! 按生命周期阶段拆分到兄弟模块：
//!
//! - [`super::prepare`] —— 发送方哈希准备
//! - [`super::send`]    —— 发送方 Offer / 暂停 / 取消
//! - [`super::receive`] —— 接收方 accept / reject / 暂停 / 取消 + IncomingTransferRuntime 接收 helper
//! - [`super::resume`]  —— 双侧断点续传 + IncomingTransferRuntime 续传 helper

use std::sync::Arc;
use std::time::Instant;

use dashmap::{DashMap, DashSet};
use sea_orm::DatabaseConnection;
use serde::Serialize;
use swarm_p2p_core::libp2p::PeerId;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use uuid::Uuid;

use crate::host::{EventBus, FileAccess, FileSourceId};
use crate::network::TransferRuntime;
use crate::protocol::{AppNetClient, FileChecksum, FileInfo, TransferResponse};
use crate::transfer::incoming::{IncomingTransferRuntime, TransferCompleteOutcome};
use crate::transfer::progress::TransferFailedEvent;
use crate::transfer::receiver::ReceiveSession;
use crate::transfer::sender::SendSession;
use crate::AppResult;

/// 发送方准备好的传输信息
#[derive(Debug, Clone)]
pub struct PreparedTransfer {
    pub prepared_id: Uuid,
    pub files: Vec<PreparedFile>,
    pub total_size: u64,
    pub created_at: Instant,
}

/// 准备好的单个文件
#[derive(Debug, Clone)]
pub struct PreparedFile {
    pub file_id: u32,
    pub name: String,
    pub relative_path: String,
    pub source_id: FileSourceId,
    pub size: u64,
    pub checksum: String,
}

/// 接收方缓存的入站 Offer
#[derive(Debug)]
pub struct PendingOffer {
    pub pending_id: u64,
    pub peer_id: PeerId,
    pub peer_name: String,
    pub session_id: Uuid,
    pub files: Vec<FileInfo>,
    pub total_size: u64,
    pub created_at: Instant,
}

/// 发送方已发出、仍在等待对端 OfferResult 的请求。
#[derive(Debug, Clone, Copy)]
pub(super) struct PendingOutboundOffer {
    pub prepared_id: Uuid,
}

/// `send_offer` 的返回类型
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct StartSendResult {
    pub session_id: Uuid,
}

/// `initiate_resume` 的返回类型
#[derive(Debug, Clone)]
pub struct ResumeInfo {
    pub peer_id: String,
    pub peer_name: String,
    pub files: Vec<ResumeFileInfo>,
    pub total_size: i64,
    pub transferred_bytes: i64,
}

#[derive(Debug, Clone)]
pub struct ResumeFileInfo {
    pub file_id: i32,
    pub name: String,
    pub relative_path: String,
    pub size: i64,
}

const PREPARED_TIMEOUT_SECS: u64 = 300;
const PENDING_OFFER_TIMEOUT_SECS: u64 = 300;
const SEND_SESSION_IDLE_TIMEOUT_MS: u64 = 30 * 60 * 1000;
const CLEANUP_INTERVAL_SECS: u64 = 60;

/// 传输管理器
///
/// 字段对兄弟模块（`prepare` / `send` / `receive` / `resume`）开放（`pub(super)`），
/// 这样它们可以挂载额外的 `impl TransferManager { ... }` 块直接访问字段。
pub struct TransferManager {
    pub(super) client: AppNetClient,
    pub(super) event_bus: Arc<dyn EventBus>,
    pub(super) db: Arc<DatabaseConnection>,
    /// 默认文件访问 trait（用于发送方读源文件、接收方写入；host 在调用时也可针对单次会话覆盖）
    pub(super) file_access: Arc<dyn FileAccess>,

    pub(super) prepared: DashMap<Uuid, PreparedTransfer>,
    pub(super) pending: DashMap<Uuid, PendingOffer>,
    /// 本端发起、仍在等待对端接受/拒绝响应的 Offer。
    pub(super) outbound_offers: DashMap<Uuid, PendingOutboundOffer>,
    /// 用户已取消、但底层 request 还未返回的 outbound Offer。
    pub(super) cancelled_outbound_offers: DashSet<Uuid>,
    pub(super) send_sessions: DashMap<Uuid, Arc<SendSession>>,
    pub(super) receive_sessions: Arc<DashMap<Uuid, Arc<ReceiveSession>>>,
}

impl TransferManager {
    pub fn new(
        client: AppNetClient,
        event_bus: Arc<dyn EventBus>,
        db: Arc<DatabaseConnection>,
        file_access: Arc<dyn FileAccess>,
    ) -> Self {
        Self {
            client,
            event_bus,
            db,
            file_access,
            prepared: DashMap::new(),
            pending: DashMap::new(),
            outbound_offers: DashMap::new(),
            cancelled_outbound_offers: DashSet::new(),
            send_sessions: DashMap::new(),
            receive_sessions: Arc::new(DashMap::new()),
        }
    }

    /// 启动后台定时清理任务
    pub fn spawn_cleanup_task(self: &Arc<Self>, cancel_token: CancellationToken) {
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(std::time::Duration::from_secs(CLEANUP_INTERVAL_SECS));
            loop {
                tokio::select! {
                    _ = cancel_token.cancelled() => {
                        info!("传输资源清理任务已停止");
                        break;
                    }
                    _ = interval.tick() => {
                        this.run_cleanup();
                    }
                }
            }
        });
    }

    fn run_cleanup(&self) {
        let now = Instant::now();
        remove_expired(
            &self.prepared,
            |v| now.duration_since(v.created_at).as_secs() > PREPARED_TIMEOUT_SECS,
            "prepared transfers",
        );
        remove_expired(
            &self.pending,
            |v| now.duration_since(v.created_at).as_secs() > PENDING_OFFER_TIMEOUT_SECS,
            "pending offers",
        );
        let idle_ids: Vec<Uuid> = self
            .send_sessions
            .iter()
            .filter(|r| r.value().idle_ms() > SEND_SESSION_IDLE_TIMEOUT_MS)
            .map(|r| *r.key())
            .collect();
        for id in &idle_ids {
            if let Some((_, session)) = self.send_sessions.remove(id) {
                session.cancel();
                warn!("清理空闲超时的 send session: {}", id);
            }
        }
    }

    pub fn client(&self) -> &AppNetClient {
        &self.client
    }

    pub fn db(&self) -> &Arc<DatabaseConnection> {
        &self.db
    }

    pub fn event_bus(&self) -> &Arc<dyn EventBus> {
        &self.event_bus
    }

    pub fn file_access(&self) -> &Arc<dyn FileAccess> {
        &self.file_access
    }
}

pub fn generate_id() -> Uuid {
    Uuid::new_v4()
}

impl TransferRuntime for TransferManager {
    fn spawn_cleanup_task(transfer: &Arc<Self>, cancel_token: CancellationToken) {
        transfer.spawn_cleanup_task(cancel_token);
    }
}

#[async_trait::async_trait]
impl IncomingTransferRuntime for TransferManager {
    async fn handle_chunk_request(
        &self,
        session_id: Uuid,
        file_id: u32,
        chunk_index: u32,
    ) -> AppResult<TransferResponse> {
        self.handle_chunk_request_impl(session_id, file_id, chunk_index)
            .await
    }

    async fn handle_complete(&self, session_id: Uuid) -> AppResult<TransferCompleteOutcome> {
        self.handle_complete_impl(session_id).await
    }

    async fn handle_cancel(
        &self,
        session_id: Uuid,
        reason: String,
    ) -> AppResult<TransferFailedEvent> {
        self.handle_cancel_impl(session_id, reason).await
    }

    async fn handle_pause(
        &self,
        session_id: Uuid,
    ) -> AppResult<crate::transfer::progress::TransferPausedEvent> {
        self.handle_pause_impl(session_id).await
    }

    fn is_paired(&self, _peer_id: &PeerId) -> bool {
        // PairingManager 不在 TransferManager 持有；caller (event_loop) 在调用前已校验。
        // 默认返回 true，避免双重校验。
        true
    }

    fn paired_device_name(&self, _peer_id: &PeerId) -> Option<String> {
        None
    }

    fn cache_inbound_offer(
        &self,
        pending_id: u64,
        peer_id: PeerId,
        device_name: String,
        session_id: Uuid,
        files: Vec<FileInfo>,
        total_size: u64,
    ) {
        TransferManager::cache_inbound_offer(
            self,
            pending_id,
            peer_id,
            device_name,
            session_id,
            files,
            total_size,
        );
    }

    async fn handle_resume_request(
        &self,
        peer_id: PeerId,
        session_id: Uuid,
        file_checksums: Vec<FileChecksum>,
    ) -> AppResult<TransferResponse> {
        self.handle_resume_request_impl(peer_id, session_id, file_checksums)
            .await
    }

    async fn handle_resume_offer(
        &self,
        peer_id: PeerId,
        session_id: Uuid,
        key: [u8; 32],
        file_checksums: Vec<FileChecksum>,
    ) -> AppResult<TransferResponse> {
        self.handle_resume_offer_impl(peer_id, session_id, key, file_checksums)
            .await
    }
}

fn remove_expired<V>(map: &DashMap<Uuid, V>, is_expired: impl Fn(&V) -> bool, label: &str) {
    let expired: Vec<Uuid> = map
        .iter()
        .filter(|r| is_expired(r.value()))
        .map(|r| *r.key())
        .collect();
    for id in &expired {
        map.remove(id);
    }
    if !expired.is_empty() {
        info!("清理 {} 个过期的 {}", expired.len(), label);
    }
}
