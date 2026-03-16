//! 传输管理器
//!
//! 管理 Offer 协议（发送、接受、拒绝）和活跃传输会话（发送/接收）。
//! 事件循环写入缓存 → 前端操作后通过 Tauri 命令消费缓存。

use std::sync::Arc;
use std::time::Instant;

use dashmap::DashMap;
use serde::Serialize;
use swarm_p2p_core::libp2p::PeerId;
use tauri::AppHandle;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use uuid::Uuid;

use sea_orm::{DatabaseConnection, EntityTrait};
use tauri::Manager;

use tauri::Emitter;

use crate::file_sink::FileSink;
use crate::file_source::{EnumeratedFile, FileSource};
use crate::protocol::{
    AppNetClient, AppRequest, AppResponse, FileChecksum, FileInfo, OfferRejectReason,
    ResumeRejectReason, TransferRequest, TransferResponse,
};
use crate::transfer::crypto::generate_key;
use crate::transfer::progress::{TransferDbErrorEvent, TransferDirection, TransferFailedEvent};
use crate::transfer::receiver::ReceiveSession;
use crate::transfer::sender::SendSession;
use crate::{events, AppError, AppResult};

/// prepare_send 进度事件（通过 Tauri Channel 实时推送给前端）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareProgress {
    /// 当前正在 hash 的文件名
    pub current_file: String,
    /// 已完成 hash 的文件数
    pub completed_files: u32,
    /// 总文件数
    pub total_files: u32,
    /// 累积已 hash 的字节数（所有文件）
    pub bytes_hashed: u64,
    /// 总字节数（所有文件）
    pub total_bytes: u64,
}

/// 发送方准备好的传输信息
#[derive(Debug, Clone)]
pub struct PreparedTransfer {
    /// 唯一标识符
    pub prepared_id: Uuid,
    /// 文件列表（含 BLAKE3 校验和）
    pub files: Vec<PreparedFile>,
    /// 总大小（字节）
    pub total_size: u64,
    /// 创建时间（用于超时清理）
    pub created_at: Instant,
}

/// 准备好的单个文件
#[derive(Debug, Clone)]
pub struct PreparedFile {
    /// 文件标识符
    pub file_id: u32,
    /// 文件名
    pub name: String,
    /// 相对路径
    pub relative_path: String,
    /// 文件来源（发送时读取文件用）
    pub source: FileSource,
    /// 文件大小
    pub size: u64,
    /// BLAKE3 校验和（hex）
    pub checksum: String,
}

/// 接收方缓存的入站 Offer
#[derive(Debug)]
pub struct PendingOffer {
    /// libp2p pending request id（回复时使用）
    pending_id: u64,
    /// 发送方 PeerId
    pub peer_id: PeerId,
    /// 对端设备名
    pub peer_name: String,
    /// 传输会话 ID
    pub session_id: Uuid,
    /// 文件列表
    pub files: Vec<FileInfo>,
    /// 总大小
    pub total_size: u64,
    /// 创建时间（用于超时清理）
    pub created_at: Instant,
}

/// `send_offer` 的返回类型（立即返回 session_id，后续通过事件通知结果）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartSendResult {
    pub session_id: Uuid,
}

/// 对方接受 Offer 的事件 payload
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferAcceptedEvent {
    pub session_id: Uuid,
}

/// 对方拒绝 Offer 的事件 payload
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferRejectedEvent {
    pub session_id: Uuid,
    pub reason: Option<OfferRejectReason>,
}

/// `initiate_resume` 的返回类型（供前端创建运行时 session）
#[derive(Debug, Clone)]
pub struct ResumeInfo {
    pub peer_id: String,
    pub peer_name: String,
    pub files: Vec<ResumeFileInfo>,
    pub total_size: i64,
    pub transferred_bytes: i64,
}

/// 恢复传输中的单个文件信息
#[derive(Debug, Clone)]
pub struct ResumeFileInfo {
    pub file_id: i32,
    pub name: String,
    pub relative_path: String,
    pub size: i64,
}

/// 超时配置常量
const PREPARED_TIMEOUT_SECS: u64 = 300; // 5 分钟
const PENDING_OFFER_TIMEOUT_SECS: u64 = 300; // 5 分钟
const SEND_SESSION_IDLE_TIMEOUT_MS: u64 = 30 * 60 * 1000; // 30 分钟
const CLEANUP_INTERVAL_SECS: u64 = 60; // 每 60 秒扫描一次

/// 传输管理器（原 OfferManager，扩展为管理完整传输生命周期）
pub struct TransferManager {
    /// libp2p 网络客户端
    client: AppNetClient,
    /// 发送方：prepare_send 的缓存（key = prepared_id）
    prepared: DashMap<Uuid, PreparedTransfer>,
    /// 接收方：入站 Offer 的缓存（key = session_id）
    pending: DashMap<Uuid, PendingOffer>,
    /// 活跃的发送会话（key = session_id）
    send_sessions: DashMap<Uuid, Arc<SendSession>>,
    /// 活跃的接收会话（key = session_id, Arc 包装以便回调中清理）
    receive_sessions: Arc<DashMap<Uuid, Arc<ReceiveSession>>>,
}

impl TransferManager {
    pub fn new(client: AppNetClient) -> Self {
        Self {
            client,
            prepared: DashMap::new(),
            pending: DashMap::new(),
            send_sessions: DashMap::new(),
            receive_sessions: Arc::new(DashMap::new()),
        }
    }

    /// 启动后台定时清理任务（在 Arc<Self> 上调用，由 NetManager 创建后触发）
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

    /// 执行一次清理扫描
    fn run_cleanup(&self) {
        let now = Instant::now();

        remove_expired(&self.prepared, |v| {
            now.duration_since(v.created_at).as_secs() > PREPARED_TIMEOUT_SECS
        }, "prepared transfers");

        remove_expired(&self.pending, |v| {
            now.duration_since(v.created_at).as_secs() > PENDING_OFFER_TIMEOUT_SECS
        }, "pending offers");

        // 清理空闲超时的 send sessions（需要额外 cancel 操作）
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

    // ============ 准备阶段 ============

    /// 准备发送：对预扫描的文件列表计算 BLAKE3 校验和、分配 fileId
    ///
    /// 接收 `scan_sources` 命令返回的 `EnumeratedFile` 列表。
    /// 前端可能已移除部分文件（用户在 UI 中取消选择）。
    /// 此方法不做目录遍历，只对每个文件计算 hash。
    /// 通过 `on_progress` Channel 实时上报字节级进度。
    pub async fn prepare(
        &self,
        entries: Vec<EnumeratedFile>,
        app: &AppHandle,
        on_progress: tauri::ipc::Channel<PrepareProgress>,
    ) -> AppResult<PreparedTransfer> {
        if entries.is_empty() {
            return Err(AppError::Transfer("文件列表为空".into()));
        }

        let total_files = entries.len() as u32;
        let total_bytes: u64 = entries.iter().map(|e| e.size).sum();
        let mut files = Vec::new();
        let mut completed_bytes: u64 = 0;

        for (file_id, entry) in entries.into_iter().enumerate() {
            let file_name = entry.name.clone();
            let base_bytes = completed_bytes;
            let completed_files = file_id as u32;
            let progress = on_progress.clone();

            let checksum = entry
                .source
                .compute_hash_with_progress(app, move |bytes_in_file| {
                    let _ = progress.send(PrepareProgress {
                        current_file: file_name.clone(),
                        completed_files,
                        total_files,
                        bytes_hashed: base_bytes + bytes_in_file,
                        total_bytes,
                    });
                })
                .await?;

            completed_bytes += entry.size;
            files.push(PreparedFile {
                file_id: file_id as u32,
                name: entry.name,
                relative_path: entry.relative_path,
                source: entry.source,
                size: entry.size,
                checksum,
            });
        }

        // 最终完成事件
        let _ = on_progress.send(PrepareProgress {
            current_file: String::new(),
            completed_files: total_files,
            total_files,
            bytes_hashed: total_bytes,
            total_bytes,
        });

        let prepared = PreparedTransfer {
            prepared_id: generate_id(),
            files,
            total_size: total_bytes,
            created_at: Instant::now(),
        };

        self.prepared.insert(prepared.prepared_id, prepared.clone());

        Ok(prepared)
    }

    // ============ 发送方：发送 Offer + 启动传输 ============

    /// 发送 Offer 到目标 peer（非阻塞）
    ///
    /// 立即返回 session_id。后台任务等待对方回复后：
    /// - 接受 → 创建 SendSession + emit `transfer-accepted`
    /// - 拒绝 → emit `transfer-rejected`
    /// - 错误 → emit `transfer-failed`
    pub fn send_offer(
        self: &Arc<Self>,
        prepared_id: &Uuid,
        peer_id: &str,
        peer_name: &str,
        selected_file_ids: &[u32],
        app: AppHandle,
    ) -> AppResult<StartSendResult> {
        let prepared = self
            .prepared
            .get(prepared_id)
            .map(|r| r.value().clone())
            .ok_or_else(|| {
                AppError::Transfer(format!("PreparedTransfer not found: {prepared_id}"))
            })?;

        // 筛选选中的文件
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
            .map(|f| source_path_string(&f.source))
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

        // 后台任务：发送 Offer 请求并等待响应
        let client = self.client.clone();
        let this = Arc::clone(self);
        let prepared_id = *prepared_id;
        let peer_id_str = peer_id.to_string();
        let peer_name = peer_name.to_string();
        tokio::spawn(async move {
            let emit_fail = |error: String| {
                let _ = app.emit(
                    events::TRANSFER_FAILED,
                    TransferFailedEvent {
                        session_id,
                        direction: TransferDirection::Send,
                        error,
                    },
                );
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

                    if let Some(db) = app.try_state::<DatabaseConnection>() {
                        if let Err(e) = crate::database::ops::create_session(
                            &db,
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
                            let _ = app.emit(
                                events::TRANSFER_DB_ERROR,
                                TransferDbErrorEvent {
                                    session_id,
                                    message: format!("保存传输记录失败: {e}"),
                                },
                            );
                        }
                    }

                    let send_session = Arc::new(SendSession::new(
                        session_id,
                        target_peer,
                        selected_prepared,
                        &key,
                        app.clone(),
                    ));
                    this.send_sessions.insert(session_id, send_session);
                    this.prepared.remove(&prepared_id);

                    let _ = app.emit(
                        events::TRANSFER_ACCEPTED,
                        TransferAcceptedEvent { session_id },
                    );
                }
                Ok(AppResponse::Transfer(TransferResponse::OfferResult {
                    accepted: false,
                    reason,
                    ..
                })) => {
                    info!("Offer rejected for session {}: {:?}", session_id, reason);
                    let _ = app.emit(
                        events::TRANSFER_REJECTED,
                        TransferRejectedEvent { session_id, reason },
                    );
                }
                Ok(AppResponse::Transfer(TransferResponse::OfferResult {
                    accepted: true,
                    key: None,
                    ..
                })) => {
                    warn!("Offer accepted 但未收到密钥: session={}", session_id);
                    emit_fail("对方接受但未提供加密密钥".into());
                }
                Ok(other) => {
                    warn!("意外的响应类型: {:?}", other);
                    emit_fail(format!("意外的响应类型: {other:?}"));
                }
                Err(e) => {
                    warn!("发送 Offer 失败: {}", e);
                    emit_fail(format!("发送 Offer 失败: {e}"));
                }
            }
        });

        Ok(StartSendResult { session_id })
    }

    // ============ 发送方：响应 ChunkRequest ============

    /// 获取发送会话（事件循环调用）
    pub fn get_send_session(&self, session_id: &Uuid) -> Option<Arc<SendSession>> {
        self.send_sessions
            .get(session_id)
            .map(|r| Arc::clone(r.value()))
    }

    /// 注册外部创建的发送会话（断点续传时由 event_loop 创建后注册）
    pub fn insert_send_session(&self, session_id: Uuid, session: Arc<SendSession>) {
        self.send_sessions.insert(session_id, session);
    }

    /// 移除发送会话
    pub fn remove_send_session(&self, session_id: &Uuid) {
        self.send_sessions.remove(session_id);
    }

    // ============ 接收方：缓存 + 响应 + 启动传输 ============

    /// 缓存入站 Offer（事件循环调用）
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

    /// 接受传输并启动接收：生成密钥、回复 OfferResult、创建 ReceiveSession 并开始拉取
    pub async fn accept_and_start_receive(
        &self,
        session_id: &Uuid,
        save_location: entity::SaveLocation,
        app: AppHandle,
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

        // 持久化接收方会话记录到 DB
        let peer_id_str = offer.peer_id.to_string();
        if let Some(db) = app.try_state::<DatabaseConnection>() {
            if let Err(e) = crate::database::ops::create_session(
                &db,
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
                let _ = app.emit(
                    events::TRANSFER_DB_ERROR,
                    TransferDbErrorEvent {
                        session_id: offer.session_id,
                        message: format!("保存传输记录失败: {e}"),
                    },
                );
            }
        }

        // 根据 SaveLocation 构造 FileSink 并启动接收
        let sink = build_file_sink(&save_location);
        self.start_receive_from_offer(
            offer.session_id,
            offer.peer_id,
            offer.files,
            offer.total_size,
            sink,
            &key,
            app,
            std::collections::HashMap::new(),
        );

        Ok(())
    }

    /// 拒绝传输：回复拒绝的 OfferResult
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

    // ============ 取消 ============

    /// 暂停发送：取消 → 通知对端 → 保存进度到 DB → 移除 SendSession
    pub async fn pause_send(&self, session_id: &Uuid, app: &AppHandle) -> AppResult<()> {
        // 1. 先取消 session（仍在 DashMap 中），立即拒绝新的 ChunkRequest，减少竞态窗口
        {
            let session = self
                .send_sessions
                .get(session_id)
                .ok_or_else(|| AppError::Transfer(format!("发送会话不存在: {session_id}")))?;
            session.cancel();
        }

        // 2. 移除 session 获取所有权
        let (_, session) = self
            .send_sessions
            .remove(session_id)
            .ok_or_else(|| AppError::Transfer(format!("发送会话不存在: {session_id}")))?;

        // 3. 通知对端（接收方）暂停
        let _ = self
            .client
            .send_request(
                session.peer_id,
                AppRequest::Transfer(TransferRequest::Pause {
                    session_id: *session_id,
                }),
            )
            .await;

        // 4. 保存发送方 per-file 进度到 DB（断点续传恢复时使用）
        if let Some(db) = app.try_state::<sea_orm::DatabaseConnection>() {
            let progress = session.get_file_progress();
            let _ = crate::database::ops::save_sender_file_progress(&db, *session_id, &progress).await;
        }

        info!("Send session paused: session={}", session_id);
        Ok(())
    }

    /// 暂停接收：取消本地 ReceiveSession → 通知对端
    pub async fn pause_receive(&self, session_id: &Uuid) -> AppResult<()> {
        let session = self
            .receive_sessions
            .get(session_id)
            .map(|r| Arc::clone(r.value()))
            .ok_or_else(|| AppError::Transfer(format!("接收会话不存在: {session_id}")))?;

        // 先停止本地接收（确保 bitmap 刷写完成）
        session.cancel_and_wait().await;

        // 从 DashMap 中移除（on_finish 回调可能已移除，这里确保清理）
        self.receive_sessions.remove(session_id);

        // 通知对端（发送方）暂停
        let _ = self
            .client
            .send_request(
                session.peer_id,
                AppRequest::Transfer(TransferRequest::Pause {
                    session_id: *session_id,
                }),
            )
            .await;

        info!("Receive session paused: session={}", session_id);
        Ok(())
    }

    /// 取消发送
    pub async fn cancel_send(&self, session_id: &Uuid) -> AppResult<()> {
        let (_, session) = self
            .send_sessions
            .remove(session_id)
            .ok_or_else(|| AppError::Transfer(format!("发送会话不存在: {session_id}")))?;

        session.cancel();
        info!("Send session cancelled: session={}", session_id);
        Ok(())
    }

    /// 取消接收
    pub async fn cancel_receive(&self, session_id: &Uuid) -> AppResult<()> {
        let session = self
            .receive_sessions
            .get(session_id)
            .map(|r| Arc::clone(r.value()))
            .ok_or_else(|| AppError::Transfer(format!("接收会话不存在: {session_id}")))?;

        // 取消并等待后台任务完成（含 bitmap 刷写），on_finish 回调会自动从 DashMap 移除
        session.cancel_and_wait().await;
        session.send_cancel().await;
        session.cleanup_part_files().await;
        info!("Receive session cancelled: session={}", session_id);
        Ok(())
    }

    /// 获取接收会话（事件循环调用）
    pub fn get_receive_session(&self, session_id: &Uuid) -> Option<Arc<ReceiveSession>> {
        self.receive_sessions
            .get(session_id)
            .map(|r| Arc::clone(r.value()))
    }

    /// 移除接收会话
    pub fn remove_receive_session(&self, session_id: &Uuid) {
        self.receive_sessions.remove(session_id);
    }

    // ============ 断点续传 ============

    /// 接收方发起断点续传，成功时返回 ResumeInfo 供前端创建运行时 session
    pub async fn initiate_resume(
        &self,
        db: &DatabaseConnection,
        session_id: Uuid,
        app: AppHandle,
    ) -> AppResult<ResumeInfo> {
        let (session, target_peer) = load_resumable_session(db, session_id).await?;
        let files = crate::database::ops::get_session_files(db, session_id).await?;
        let file_checksums = build_file_checksums(&files);

        info!(
            "发起断点续传: session={}, files={}",
            session_id,
            file_checksums.len()
        );

        // 发送 ResumeRequest
        let response = self
            .client
            .send_request(
                target_peer,
                AppRequest::Transfer(TransferRequest::ResumeRequest {
                    session_id,
                    file_checksums,
                }),
            )
            .await
            .map_err(|e| AppError::Transfer(format!("ResumeRequest 发送失败: {e}")))?;

        match response {
            AppResponse::Transfer(TransferResponse::ResumeResult {
                accepted: true,
                key: Some(key),
                ..
            }) => {
                info!("Resume accepted for session {}", session_id);

                crate::database::ops::mark_session_transferring(db, session_id).await?;

                let total_size = session.total_size;
                let save_location = session.save_path.unwrap_or(entity::SaveLocation::Path {
                    path: String::new(),
                });
                let peer_id = session.peer_id.0;
                let peer_name = session.peer_name;

                let (file_infos, initial_bitmaps) = build_file_infos_and_bitmaps(&files);
                let (resume_file_infos, transferred_bytes) = build_resume_file_infos(&files);

                self.start_receive_from_offer(
                    session_id,
                    target_peer,
                    file_infos,
                    total_size as u64,
                    build_file_sink(&save_location),
                    &key,
                    app,
                    initial_bitmaps,
                );

                Ok(ResumeInfo {
                    peer_id,
                    peer_name,
                    files: resume_file_infos,
                    total_size,
                    transferred_bytes,
                })
            }
            AppResponse::Transfer(TransferResponse::ResumeResult {
                accepted: true,
                key: None,
                ..
            }) => Err(AppError::Transfer("Resume accepted 但未收到密钥".into())),
            AppResponse::Transfer(TransferResponse::ResumeResult {
                accepted: false,
                reason: Some(ResumeRejectReason::SenderCancelled),
                ..
            }) => {
                info!(
                    "Resume rejected for session {}: 发送方已取消传输",
                    session_id
                );
                crate::database::ops::mark_session_cancelled(db, session_id).await?;
                Err(AppError::Transfer("发送方已取消传输".into()))
            }
            AppResponse::Transfer(TransferResponse::ResumeResult {
                accepted: false,
                reason,
                ..
            }) => {
                let reason_str = match reason {
                    Some(ResumeRejectReason::FileModified) => "源文件已被修改，无法恢复传输",
                    Some(ResumeRejectReason::SessionNotFound) => "发送方找不到对应会话",
                    _ => "未知原因",
                };
                info!("Resume rejected for session {}: {}", session_id, reason_str);
                crate::database::ops::mark_session_failed(db, session_id, reason_str).await?;
                Err(AppError::Transfer(reason_str.into()))
            }
            other => Err(AppError::Transfer(format!("意外的响应类型: {other:?}"))),
        }
    }

    /// 发送方发起断点续传：重建 SendSession → 发送 ResumeOffer → 接收方创建 ReceiveSession
    pub async fn initiate_resume_as_sender(
        &self,
        db: &DatabaseConnection,
        session_id: Uuid,
        app: AppHandle,
    ) -> AppResult<ResumeInfo> {
        let (session, target_peer) = load_resumable_session(db, session_id).await?;
        let files = crate::database::ops::get_session_files(db, session_id).await?;

        // 重建 PreparedFile 并验证源文件
        let prepared_files = build_prepared_files_from_db(&files).await?;
        let file_checksums = build_file_checksums(&files);
        let (resume_file_infos, _) = build_resume_file_infos(&files);

        // 生成密钥
        let key = generate_key();

        info!(
            "发送方发起断点续传: session={}, files={}",
            session_id,
            file_checksums.len()
        );

        // 从 DB 构建 resume_state
        let resume_state = build_sender_resume_state(&files);

        // 先创建 SendSession 并插入 DashMap（接收方开始 pulling 前必须就绪）
        let send_session = Arc::new(SendSession::new_with_resume(
            session_id,
            target_peer,
            prepared_files,
            &key,
            app,
            &resume_state,
        ));
        self.send_sessions.insert(session_id, send_session);

        // 发送 ResumeOffer 给接收方
        let response = self
            .client
            .send_request(
                target_peer,
                AppRequest::Transfer(TransferRequest::ResumeOffer {
                    session_id,
                    key,
                    file_checksums,
                }),
            )
            .await
            .map_err(|e| {
                self.send_sessions.remove(&session_id);
                AppError::Transfer(format!("ResumeOffer 发送失败: {e}"))
            })?;

        match response {
            AppResponse::Transfer(TransferResponse::ResumeOfferResult {
                accepted: true, ..
            }) => {
                info!("ResumeOffer accepted for session {}", session_id);

                crate::database::ops::mark_session_transferring(db, session_id).await?;

                let transferred_bytes: i64 = files.iter().map(|f| f.transferred_bytes).sum();

                Ok(ResumeInfo {
                    peer_id: session.peer_id.0,
                    peer_name: session.peer_name,
                    files: resume_file_infos,
                    total_size: session.total_size,
                    transferred_bytes,
                })
            }
            AppResponse::Transfer(TransferResponse::ResumeOfferResult {
                accepted: false,
                reason,
                ..
            }) => {
                self.send_sessions.remove(&session_id);

                let reason_str = match reason {
                    Some(ResumeRejectReason::FileModified) => "接收方文件校验不匹配",
                    Some(ResumeRejectReason::SessionNotFound) => "接收方找不到对应会话",
                    Some(ResumeRejectReason::SenderCancelled) => "接收方已取消传输",
                    None => "未知原因",
                };
                info!(
                    "ResumeOffer rejected for session {}: {}",
                    session_id, reason_str
                );
                crate::database::ops::mark_session_failed(db, session_id, reason_str).await?;
                Err(AppError::Transfer(reason_str.into()))
            }
            other => {
                self.send_sessions.remove(&session_id);
                Err(AppError::Transfer(format!("意外的响应类型: {other:?}")))
            }
        }
    }

    /// 获取网络客户端（供 event_loop 中处理 ResumeRequest 时使用）
    pub fn client(&self) -> &AppNetClient {
        &self.client
    }

    /// 创建 ReceiveSession 并开始拉取
    #[expect(clippy::too_many_arguments, reason = "传输会话初始化需要完整上下文")]
    pub fn start_receive_from_offer(
        &self,
        session_id: Uuid,
        peer_id: PeerId,
        files: Vec<FileInfo>,
        total_size: u64,
        sink: FileSink,
        key: &[u8; 32],
        app: AppHandle,
        initial_bitmaps: std::collections::HashMap<u32, Vec<u8>>,
    ) {
        let receive_session = Arc::new(ReceiveSession::new(
            session_id,
            peer_id,
            files,
            total_size,
            sink,
            key,
            self.client.clone(),
            app,
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

/// 生成随机的 session/prepared ID（UUID v4）
pub fn generate_id() -> Uuid {
    Uuid::new_v4()
}

/// 将 `FileSource` 转换为可持久化的路径字符串
fn source_path_string(source: &FileSource) -> String {
    match source {
        FileSource::Path { path } => path.to_string_lossy().into_owned(),
        #[cfg(target_os = "android")]
        FileSource::AndroidUri(uri) => serde_json::to_string(uri).unwrap_or_default(),
    }
}

/// 根据 SaveLocation 构造 FileSink
pub(crate) fn build_file_sink(save_location: &entity::SaveLocation) -> FileSink {
    match save_location {
        entity::SaveLocation::Path { path } => FileSink::Path {
            save_dir: std::path::PathBuf::from(path),
        },
        #[cfg(target_os = "android")]
        entity::SaveLocation::AndroidPublicDir { subdir } => FileSink::AndroidPublicDir {
            subdir: subdir.clone(),
        },
        #[cfg(not(target_os = "android"))]
        entity::SaveLocation::AndroidPublicDir { .. } => {
            unreachable!("AndroidPublicDir 不应出现在非 Android 平台")
        }
    }
}

/// 从 DB 文件记录构建发送方的 resume_state（file_id → (chunks_done, transferred_bytes)）
///
/// 利用 transferred_bytes 和 CHUNK_SIZE 反推 chunks_done。
pub(crate) fn build_sender_resume_state(
    files: &[entity::transfer_file::Model],
) -> std::collections::HashMap<u32, (u32, u64)> {
    use crate::file_source::{calc_total_chunks, CHUNK_SIZE};

    files
        .iter()
        .filter_map(|f| {
            let transferred = f.transferred_bytes as u64;
            if transferred == 0 {
                return None;
            }
            let file_id = f.file_id as u32;
            let file_size = f.size as u64;
            let total_chunks = calc_total_chunks(file_size);
            let chunk_size = CHUNK_SIZE as u64;

            // 反推 chunks_done：transferred 覆盖了多少个完整/部分 chunk
            // 最后一个 chunk 可能小于 CHUNK_SIZE，需要用 div_ceil 正确计算
            let chunks_done = if transferred >= file_size {
                total_chunks
            } else {
                (transferred.div_ceil(chunk_size)) as u32
            };

            Some((file_id, (chunks_done, transferred)))
        })
        .collect()
}

// ============ 断点续传辅助函数 ============

/// 解析 PeerId 字符串，失败时返回统一的传输错误
pub(crate) fn parse_peer_id(s: &str) -> AppResult<PeerId> {
    s.parse()
        .map_err(|_| AppError::Transfer(format!("无效的 PeerId: {s}")))
}

/// 从 DB 加载可恢复的会话（状态必须为 Paused 或 Failed），同时返回解析后的 PeerId
pub(crate) async fn load_resumable_session(
    db: &DatabaseConnection,
    session_id: Uuid,
) -> AppResult<(entity::transfer_session::Model, PeerId)> {
    let session = entity::TransferSession::find_by_id(session_id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::Transfer("会话不存在".into()))?;

    if !matches!(
        session.status,
        entity::SessionStatus::Paused | entity::SessionStatus::Failed
    ) {
        return Err(AppError::Transfer(format!(
            "会话状态不支持恢复: {:?}",
            session.status
        )));
    }

    let target_peer = parse_peer_id(&session.peer_id.0)?;
    Ok((session, target_peer))
}

/// 从 DB 文件记录构建 ResumeFileInfo 列表和 transferred_bytes 合计
pub(crate) fn build_resume_file_infos(
    files: &[entity::transfer_file::Model],
) -> (Vec<ResumeFileInfo>, i64) {
    let mut infos = Vec::with_capacity(files.len());
    let mut transferred_bytes: i64 = 0;
    for f in files {
        infos.push(ResumeFileInfo {
            file_id: f.file_id,
            name: f.name.clone(),
            relative_path: f.relative_path.clone(),
            size: f.size,
        });
        transferred_bytes += f.transferred_bytes;
    }
    (infos, transferred_bytes)
}

/// 从 DB 文件记录构建 FileChecksum 列表
pub(crate) fn build_file_checksums(files: &[entity::transfer_file::Model]) -> Vec<FileChecksum> {
    files
        .iter()
        .map(|f| FileChecksum {
            file_id: f.file_id as u32,
            checksum: f.checksum.clone(),
        })
        .collect()
}

/// 从 DB 文件记录构建 FileInfo 列表和 initial_bitmaps
pub(crate) fn build_file_infos_and_bitmaps(
    files: &[entity::transfer_file::Model],
) -> (Vec<FileInfo>, std::collections::HashMap<u32, Vec<u8>>) {
    let mut file_infos = Vec::with_capacity(files.len());
    let mut bitmaps = std::collections::HashMap::with_capacity(files.len());
    for f in files {
        let fid = f.file_id as u32;
        file_infos.push(FileInfo {
            file_id: fid,
            name: f.name.clone(),
            relative_path: f.relative_path.clone(),
            size: f.size as u64,
            checksum: f.checksum.clone(),
        });
        bitmaps.insert(fid, f.completed_chunks.clone());
    }
    (file_infos, bitmaps)
}

/// 从 DB 文件记录重建 PreparedFile 列表（验证源文件存在且大小匹配）
pub(crate) async fn build_prepared_files_from_db(
    files: &[entity::transfer_file::Model],
) -> AppResult<Vec<PreparedFile>> {
    let mut prepared = Vec::with_capacity(files.len());
    for f in files {
        let source_path = f.source_path.as_ref().ok_or_else(|| {
            AppError::Transfer(format!("文件缺少 source_path: file_id={}", f.file_id))
        })?;
        let path = std::path::PathBuf::from(source_path);
        match tokio::fs::metadata(&path).await {
            Ok(meta) if meta.len() == f.size as u64 => {}
            _ => {
                return Err(AppError::Transfer(format!(
                    "源文件不存在或大小不匹配: {}",
                    source_path
                )));
            }
        }
        prepared.push(PreparedFile {
            file_id: f.file_id as u32,
            name: f.name.clone(),
            relative_path: f.relative_path.clone(),
            source: FileSource::Path { path },
            size: f.size as u64,
            checksum: f.checksum.clone(),
        });
    }
    Ok(prepared)
}

/// 从 DashMap 中移除满足条件的条目并记录日志
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
