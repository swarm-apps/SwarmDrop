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

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use dashmap::{DashMap, DashSet};
use serde::Serialize;
use swarmdrop_net::{Endpoint, NodeId};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use uuid::Uuid;

use crate::AppResult;
use crate::actor::registry::ActorRegistry;
use crate::events::TransferEventSink;
use crate::host::{CoreSaveLocation, FileAccess, FileSourceId};
use crate::incoming::IncomingTransferRuntime;
use crate::policy::ReceivePolicyDecision;
use crate::progress::TransferFailedEvent;
use crate::protocol::{FileInfo, TransferResponse};
use crate::runtime::TransferRuntime;
use crate::store::TransferStore;

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
///
/// `responder` 是 transfer-ctrl RPC handler 的应答通道：handler 缓存本条 offer 后
/// await 用户决策，UI accept/reject 时 send 一个 `OfferResult` 解决它，handler 据此
/// 回复对端。清理任务回收本条目 → drop responder → handler 得 RecvError → 婉拒。
#[derive(Debug)]
pub struct PendingOffer {
    pub peer_id: NodeId,
    pub peer_name: String,
    pub session_id: Uuid,
    pub files: Vec<FileInfo>,
    pub total_size: u64,
    pub created_at: Instant,
    pub responder: oneshot::Sender<TransferResponse>,
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
/// 挂起入站 offer 的内存回收窗口。
///
/// 必须**小于**发送端 Offer 请求的真实响应窗口——后者受底层协议超时
/// `req_resp_timeout`（180s）封顶（`OFFER_RESPONSE_TIMEOUT_SECS` 的 client 侧 with_timeout
/// 加长不了，见 `flow/send.rs`）。取 170s 保证本端 pending 先于发送端 180s 放弃被回收，
/// 避免"接收端刚接受、发送端已超时放弃 → 回复通道已关"的边界竞态。有效决策窗口约 3 分钟。
const PENDING_OFFER_TIMEOUT_SECS: u64 = 170;
const SEND_SESSION_IDLE_TIMEOUT_MS: u64 = 30 * 60 * 1000;
const CLEANUP_INTERVAL_SECS: u64 = 60;

/// 传输管理器
///
/// 字段对兄弟模块（`prepare` / `send` / `receive` / `resume`）开放（`pub(crate)`），
/// 这样它们可以挂载额外的 `impl TransferManager { ... }` 块直接访问字段。
pub struct TransferManager {
    pub(crate) endpoint: Endpoint,
    pub(crate) events: Arc<dyn TransferEventSink>,
    pub(crate) store: Arc<dyn TransferStore>,
    /// 默认文件访问 trait（用于发送方读源文件、接收方写入；host 在调用时也可针对单次会话覆盖）
    pub(crate) file_access: Arc<dyn FileAccess>,
    /// 传输生命周期协调器（状态变化的统一持久化 + projection 入口）。
    pub(crate) coordinator: Arc<crate::coordinator::TransferCoordinator>,

    pub(crate) prepared: DashMap<Uuid, PreparedTransfer>,
    pub(crate) pending: DashMap<Uuid, PendingOffer>,
    /// 本端发起、仍在等待对端接受/拒绝响应的 Offer。
    pub(crate) outbound_offers: DashMap<Uuid, PendingOutboundOffer>,
    /// 用户已取消、但底层 request 还未返回的 outbound Offer。
    pub(crate) cancelled_outbound_offers: DashSet<Uuid>,
    pub(crate) actors: ActorRegistry,
    /// 全局「暂停接收」开关。运行时态、不持久化（重启回到「接收中」）。
    /// 暂停期间节点仍在线可发现、配对不受影响，仅对新 offer 婉拒（见 `incoming.rs`）。
    receiving_paused: AtomicBool,
}

impl TransferManager {
    pub fn new(
        endpoint: Endpoint,
        events: Arc<dyn TransferEventSink>,
        store: Arc<dyn TransferStore>,
        file_access: Arc<dyn FileAccess>,
    ) -> Self {
        let coordinator = Arc::new(crate::coordinator::TransferCoordinator::new(
            store.clone(),
            events.clone(),
        ));
        Self {
            endpoint,
            events,
            store,
            file_access,
            coordinator,
            prepared: DashMap::new(),
            pending: DashMap::new(),
            outbound_offers: DashMap::new(),
            cancelled_outbound_offers: DashSet::new(),
            actors: ActorRegistry::new(),
            receiving_paused: AtomicBool::new(false),
        }
    }

    /// 设置「暂停接收」状态。
    pub fn set_receiving_paused(&self, paused: bool) {
        self.receiving_paused.store(paused, Ordering::Relaxed);
    }

    /// 启动后台定时清理任务
    ///
    /// 入站数据面不再由本任务轮询——新内核经 Router 的 [`TransferDataHandler`]
    /// （[`wire::data_plane`](crate::wire)）按协议路由，装配在 runtime 层。
    pub fn spawn_cleanup_task(self: &Arc<Self>, cancel_token: CancellationToken) {
        let this = Arc::clone(self);
        // spawn/time 走 n0-future（native=tokio，wasm=spawn_local + web-time）；固定间隔
        // 清理不需要 tokio interval 的 fixed-rate 语义，每轮重置的 sleep 即可，且 wasm 可编。
        n0_future::task::spawn(async move {
            let interval = std::time::Duration::from_secs(CLEANUP_INTERVAL_SECS);
            loop {
                tokio::select! {
                    _ = cancel_token.cancelled() => {
                        info!("传输资源清理任务已停止");
                        break;
                    }
                    _ = n0_future::time::sleep(interval) => {
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

    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    pub fn file_access(&self) -> &Arc<dyn FileAccess> {
        &self.file_access
    }
}

impl TransferRuntime for TransferManager {
    fn spawn_cleanup_task(transfer: &Arc<Self>, cancel_token: CancellationToken) {
        transfer.spawn_cleanup_task(cancel_token);
    }
}

#[async_trait::async_trait]
impl IncomingTransferRuntime for TransferManager {
    fn is_receiving_paused(&self) -> bool {
        self.receiving_paused.load(Ordering::Relaxed)
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
    ) -> AppResult<crate::progress::TransferPausedEvent> {
        self.handle_pause_impl(session_id).await
    }

    async fn handle_peer_disconnected(&self, peer_id: NodeId) {
        self.handle_peer_disconnected_impl(peer_id).await
    }

    async fn cache_inbound_offer(
        &self,
        peer_id: NodeId,
        device_name: String,
        session_id: Uuid,
        files: Vec<FileInfo>,
        total_size: u64,
        origin: crate::protocol::TransferOrigin,
        policy_decision: ReceivePolicyDecision,
    ) -> AppResult<oneshot::Receiver<TransferResponse>> {
        TransferManager::cache_inbound_offer(
            self,
            peer_id,
            device_name,
            session_id,
            files,
            total_size,
            origin,
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
        peer_id: NodeId,
        peer_name: String,
        session_id: Uuid,
        files: Vec<FileInfo>,
        total_size: u64,
        origin: crate::protocol::TransferOrigin,
        policy_decision: ReceivePolicyDecision,
    ) -> AppResult<()> {
        TransferManager::record_rejected_inbound_offer(
            self,
            peer_id,
            peer_name,
            session_id,
            files,
            total_size,
            origin,
            policy_decision,
        )
        .await
    }

    async fn handle_resume_probe(&self, session_id: Uuid) -> AppResult<TransferResponse> {
        self.handle_resume_probe_impl(session_id).await
    }

    async fn handle_resume_commit(
        &self,
        peer_id: NodeId,
        session_id: Uuid,
        new_epoch: i64,
        fetch_plan: Vec<crate::protocol::FileRange>,
    ) -> AppResult<TransferResponse> {
        self.handle_resume_commit_impl(peer_id, session_id, new_epoch, fetch_plan)
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
