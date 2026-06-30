//! 传输管理器
//!
//! 管理 Offer 协议（发送、接受、拒绝、暂停、恢复）和活跃传输会话。
//! 文件 I/O / 事件推送 / 数据库读写全部通过 trait 抽象，host (Tauri / RN) 各自实现。
//!
//! 本文件保留 `TransferManager` 的结构体定义、构造、清理任务、访问器，以及两个
//! 顶层 trait impl（`TransferRuntime` / `IncomingTransferRuntime`）。具体业务方法
//! 按生命周期阶段拆分到兄弟模块：
//!
//! - [`super::flow::prepare`] —— 发送方哈希准备
//! - [`super::flow::send`]    —— 发送方 Offer / 暂停 / 取消
//! - [`super::flow::receive`] —— 接收方 accept / reject / 暂停 / 取消 + IncomingTransferRuntime 接收 helper
//! - [`super::flow::resume`]  —— 双侧断点续传 + IncomingTransferRuntime 续传 helper

use std::sync::{Arc, Mutex};
use std::time::Instant;

use dashmap::{DashMap, DashSet};
use sea_orm::DatabaseConnection;
use serde::Serialize;
use swarm_p2p_core::DataChannelReceiver;
use swarm_p2p_core::libp2p::PeerId;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use uuid::Uuid;

use crate::AppResult;
use crate::host::{CoreSaveLocation, EventBus, FileAccess, FileSourceId};
use crate::network::TransferRuntime;
use crate::protocol::{AppNetClient, FileInfo, TransferResponse};
use crate::transfer::actor::registry::ActorRegistry;
use crate::transfer::incoming::IncomingTransferRuntime;
use crate::transfer::policy::ReceivePolicyDecision;
use crate::transfer::progress::TransferFailedEvent;

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

/// 协议 `FileInfo` 的两个唯一构造来源（发送方准备表 / DB 行），集中转换避免逐字段
/// 复制散落（send / sender / resume 多处）。
impl From<&PreparedFile> for FileInfo {
    fn from(f: &PreparedFile) -> Self {
        Self {
            file_id: f.file_id,
            name: f.name.clone(),
            relative_path: f.relative_path.clone(),
            size: f.size,
            checksum: f.checksum.clone(),
        }
    }
}

impl From<&entity::transfer_file::Model> for FileInfo {
    fn from(f: &entity::transfer_file::Model) -> Self {
        Self {
            file_id: f.file_id as u32,
            name: f.name.clone(),
            relative_path: f.relative_path.clone(),
            size: f.size as u64,
            checksum: f.checksum.clone(),
        }
    }
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
pub(crate) struct PendingOutboundOffer {
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
/// 字段对兄弟模块（`prepare` / `send` / `receive` / `resume`）开放（`pub(crate)`），
/// 这样它们可以挂载额外的 `impl TransferManager { ... }` 块直接访问字段。
pub struct TransferManager {
    pub(crate) client: AppNetClient,
    pub(crate) event_bus: Arc<dyn EventBus>,
    pub(crate) db: Arc<DatabaseConnection>,
    /// 默认文件访问 trait（用于发送方读源文件、接收方写入；host 在调用时也可针对单次会话覆盖）
    pub(crate) file_access: Arc<dyn FileAccess>,
    /// 传输生命周期协调器（状态变化的统一持久化 + projection 入口）。
    pub(crate) coordinator: Arc<crate::transfer::coordinator::TransferCoordinator>,

    pub(crate) prepared: DashMap<Uuid, PreparedTransfer>,
    pub(crate) pending: DashMap<Uuid, PendingOffer>,
    /// 本端发起、仍在等待对端接受/拒绝响应的 Offer。
    pub(crate) outbound_offers: DashMap<Uuid, PendingOutboundOffer>,
    /// 用户已取消、但底层 request 还未返回的 outbound Offer。
    pub(crate) cancelled_outbound_offers: DashSet<Uuid>,
    pub(crate) actors: ActorRegistry,
    /// 入站 data-channel 接收器。只在后台任务启动时取出一次。
    data_channel_rx: Mutex<Option<DataChannelReceiver>>,
}

impl TransferManager {
    pub fn new(
        client: AppNetClient,
        event_bus: Arc<dyn EventBus>,
        db: Arc<DatabaseConnection>,
        file_access: Arc<dyn FileAccess>,
        data_channel_rx: DataChannelReceiver,
    ) -> Self {
        let coordinator = Arc::new(crate::transfer::coordinator::TransferCoordinator::new(
            db.clone(),
            event_bus.clone(),
        ));
        Self {
            client,
            event_bus,
            db,
            file_access,
            coordinator,
            prepared: DashMap::new(),
            pending: DashMap::new(),
            outbound_offers: DashMap::new(),
            cancelled_outbound_offers: DashSet::new(),
            actors: ActorRegistry::new(),
            data_channel_rx: Mutex::new(Some(data_channel_rx)),
        }
    }

    /// 启动后台定时清理任务
    pub fn spawn_cleanup_task(self: &Arc<Self>, cancel_token: CancellationToken) {
        match self.data_channel_rx.lock() {
            Ok(mut rx) => {
                if let Some(rx) = rx.take() {
                    self.spawn_data_channel_task(rx, cancel_token.clone());
                }
            }
            Err(e) => warn!("data-channel receiver lock poisoned: {}", e),
        }

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
        let idle_ids = self.actors.idle_send_ids(SEND_SESSION_IDLE_TIMEOUT_MS);
        for id in &idle_ids {
            if let Some(session) = self.actors.remove_send(id) {
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

    async fn handle_peer_disconnected(&self, peer_id: PeerId) {
        self.handle_peer_disconnected_impl(peer_id).await
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
    ) -> AppResult<()> {
        TransferManager::cache_inbound_offer(
            self,
            pending_id,
            peer_id,
            device_name,
            session_id,
            files,
            total_size,
            policy_decision,
        )
        .await
    }

    async fn accept_cached_inbound_offer(
        &self,
        session_id: Uuid,
        save_location: CoreSaveLocation,
    ) -> AppResult<()> {
        self.accept_and_start_receive(&session_id, save_location)
            .await
    }

    async fn record_rejected_inbound_offer(
        &self,
        peer_id: PeerId,
        peer_name: String,
        session_id: Uuid,
        files: Vec<FileInfo>,
        total_size: u64,
        policy_decision: ReceivePolicyDecision,
    ) -> AppResult<()> {
        TransferManager::record_rejected_inbound_offer(
            self,
            peer_id,
            peer_name,
            session_id,
            files,
            total_size,
            policy_decision,
        )
        .await
    }

    async fn handle_resume_probe(&self, session_id: Uuid) -> AppResult<TransferResponse> {
        self.handle_resume_probe_impl(session_id).await
    }

    async fn handle_resume_commit(
        &self,
        peer_id: PeerId,
        session_id: Uuid,
        new_epoch: i64,
        key: [u8; 32],
        fetch_plan: Vec<crate::protocol::FileRange>,
    ) -> AppResult<TransferResponse> {
        self.handle_resume_commit_impl(peer_id, session_id, new_epoch, key, fetch_plan)
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
