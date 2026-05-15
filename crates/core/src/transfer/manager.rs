//! 传输管理器
//!
//! 管理 Offer 协议（发送、接受、拒绝、暂停、恢复）和活跃传输会话。
//! 文件 I/O / 事件推送 / 数据库读写全部通过 trait 抽象，host (Tauri / RN) 各自实现。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use dashmap::DashMap;
use sea_orm::{DatabaseConnection, EntityTrait};
use serde::Serialize;
use swarm_p2p_core::libp2p::PeerId;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use uuid::Uuid;

use crate::host::{CoreEvent, EventBus, FileAccess, FileSourceId};
use crate::network::TransferRuntime;
use crate::transfer::incoming::{IncomingTransferRuntime, TransferCompleteOutcome};
use crate::protocol::{
    AppNetClient, AppRequest, AppResponse, FileChecksum, FileInfo, OfferRejectReason,
    ResumeRejectReason, TransferRequest, TransferResponse,
};
use crate::transfer::crypto::generate_key;
use crate::transfer::progress::{
    PrepareProgressEvent, TransferAcceptedEvent, TransferDbErrorEvent, TransferDirection,
    TransferFailedEvent, TransferRejectedEvent,
};
use crate::transfer::receiver::ReceiveSession;
use crate::transfer::sender::SendSession;
use crate::transfer::{calc_total_chunks, HostEnumeratedFile, CHUNK_SIZE};
use crate::{AppError, AppResult};

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

/// `send_offer` 的返回类型
#[derive(Debug, Clone, Serialize)]
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
pub struct TransferManager {
    client: AppNetClient,
    event_bus: Arc<dyn EventBus>,
    db: Arc<DatabaseConnection>,
    /// 默认文件访问 trait（用于发送方读源文件、接收方写入；host 在调用时也可针对单次会话覆盖）
    file_access: Arc<dyn FileAccess>,

    prepared: DashMap<Uuid, PreparedTransfer>,
    pending: DashMap<Uuid, PendingOffer>,
    send_sessions: DashMap<Uuid, Arc<SendSession>>,
    receive_sessions: Arc<DashMap<Uuid, Arc<ReceiveSession>>>,
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

    // ============ 准备阶段 ============

    /// 准备发送：流式 BLAKE3 hash + 进度事件推送
    ///
    /// - `prepared_id` 由 caller 生成，便于 host 关联进度推送通道
    /// - hash 计算通过 `FileAccess::read_source_chunk` 走，平台无关
    /// - PrepareProgress 事件按 200ms 节流推送，避免大文件刷屏
    pub async fn prepare(
        &self,
        prepared_id: Uuid,
        entries: Vec<HostEnumeratedFile>,
    ) -> AppResult<PreparedTransfer> {
        if entries.is_empty() {
            return Err(AppError::Transfer("文件列表为空".into()));
        }

        let total_files = entries.len() as u32;
        let total_bytes: u64 = entries.iter().map(|e| e.size).sum();
        let mut files = Vec::with_capacity(entries.len());
        let mut completed_bytes: u64 = 0;
        let mut last_emit = Instant::now();
        const PROGRESS_THROTTLE: std::time::Duration = std::time::Duration::from_millis(200);

        for (idx, entry) in entries.into_iter().enumerate() {
            let file_id = idx as u32;
            let mut hasher = blake3::Hasher::new();

            let total_chunks = calc_total_chunks(entry.size);
            for chunk_idx in 0..total_chunks {
                let offset = chunk_idx as u64 * CHUNK_SIZE as u64;
                let remaining = entry.size.saturating_sub(offset);
                let length = (remaining as usize).min(CHUNK_SIZE);
                if length == 0 && entry.size != 0 {
                    break;
                }
                let chunk = self
                    .file_access
                    .read_source_chunk(&entry.source_id, offset, length)
                    .await?;
                hasher.update(&chunk);
                let bytes_in_file = offset + chunk.len() as u64;

                let now = Instant::now();
                if now.duration_since(last_emit) >= PROGRESS_THROTTLE {
                    last_emit = now;
                    let _ = self
                        .event_bus
                        .publish(CoreEvent::PrepareProgress {
                            event: PrepareProgressEvent {
                                prepared_id,
                                current_file: entry.name.clone(),
                                completed_files: file_id,
                                total_files,
                                bytes_hashed: completed_bytes + bytes_in_file,
                                total_bytes,
                            },
                        })
                        .await;
                }
            }

            let checksum = hasher.finalize().to_hex().to_string();
            completed_bytes += entry.size;

            files.push(PreparedFile {
                file_id,
                name: entry.name,
                relative_path: entry.relative_path,
                source_id: entry.source_id,
                size: entry.size,
                checksum,
            });
        }

        // 最终完成事件（不受节流限制）
        let _ = self
            .event_bus
            .publish(CoreEvent::PrepareProgress {
                event: PrepareProgressEvent {
                    prepared_id,
                    current_file: String::new(),
                    completed_files: total_files,
                    total_files,
                    bytes_hashed: total_bytes,
                    total_bytes,
                },
            })
            .await;

        let prepared = PreparedTransfer {
            prepared_id,
            files,
            total_size: total_bytes,
            created_at: Instant::now(),
        };

        self.prepared.insert(prepared.prepared_id, prepared.clone());
        Ok(prepared)
    }

    // ============ 发送方 ============

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
                                direction: TransferDirection::Send,
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
                    this.prepared.remove(&prepared_id);

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
                    warn!("Offer accepted 但未收到密钥: session={}", session_id);
                    publish_failed("对方接受但未提供加密密钥".into()).await;
                }
                Ok(other) => {
                    warn!("意外的响应类型: {:?}", other);
                    publish_failed(format!("意外的响应类型: {other:?}")).await;
                }
                Err(e) => {
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

    // ============ 接收方 ============

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
        save_location: entity::SaveLocation,
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

    // ============ 暂停 / 取消 ============

    pub async fn pause_send(&self, session_id: &Uuid) -> AppResult<()> {
        {
            let session = self
                .send_sessions
                .get(session_id)
                .ok_or_else(|| AppError::Transfer(format!("发送会话不存在: {session_id}")))?;
            session.cancel();
        }

        let (_, session) = self
            .send_sessions
            .remove(session_id)
            .ok_or_else(|| AppError::Transfer(format!("发送会话不存在: {session_id}")))?;

        let _ = self
            .client
            .send_request(
                session.peer_id,
                AppRequest::Transfer(TransferRequest::Pause {
                    session_id: *session_id,
                }),
            )
            .await;

        let progress = session.get_file_progress();
        let _ =
            crate::database::ops::save_sender_file_progress(&self.db, *session_id, &progress).await;

        info!("Send session paused: session={}", session_id);
        Ok(())
    }

    pub async fn pause_receive(&self, session_id: &Uuid) -> AppResult<()> {
        let session = self
            .receive_sessions
            .get(session_id)
            .map(|r| Arc::clone(r.value()))
            .ok_or_else(|| AppError::Transfer(format!("接收会话不存在: {session_id}")))?;

        session.cancel_and_wait().await;
        self.receive_sessions.remove(session_id);

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

    pub async fn cancel_send(&self, session_id: &Uuid) -> AppResult<()> {
        let (_, session) = self
            .send_sessions
            .remove(session_id)
            .ok_or_else(|| AppError::Transfer(format!("发送会话不存在: {session_id}")))?;

        session.cancel();
        info!("Send session cancelled: session={}", session_id);
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

    // ============ 断点续传 ============

    pub async fn initiate_resume(&self, session_id: Uuid) -> AppResult<ResumeInfo> {
        let (session, target_peer) = load_resumable_session(&self.db, session_id).await?;
        let files = crate::database::ops::get_session_files(&self.db, session_id).await?;
        let file_checksums = build_file_checksums(&files);

        info!(
            "发起断点续传: session={}, files={}",
            session_id,
            file_checksums.len()
        );

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
                crate::database::ops::mark_session_transferring(&self.db, session_id).await?;

                let total_size = session.total_size;
                let save_location = session.save_path.unwrap_or(entity::SaveLocation::Path {
                    path: String::new(),
                });
                let peer_id = session.peer_id.0;
                let peer_name = session.peer_name;

                let (file_infos, initial_bitmaps) = build_file_infos_and_bitmaps(&files);
                let (resume_file_infos, transferred_bytes) = build_resume_file_infos(&files);

                self.start_receive_session(
                    session_id,
                    target_peer,
                    file_infos,
                    total_size as u64,
                    save_location,
                    &key,
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
                info!("Resume rejected: 发送方已取消传输");
                crate::database::ops::mark_session_cancelled(&self.db, session_id).await?;
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
                info!("Resume rejected: {}", reason_str);
                crate::database::ops::mark_session_failed(&self.db, session_id, reason_str).await?;
                Err(AppError::Transfer(reason_str.into()))
            }
            other => Err(AppError::Transfer(format!("意外的响应类型: {other:?}"))),
        }
    }

    pub async fn initiate_resume_as_sender(&self, session_id: Uuid) -> AppResult<ResumeInfo> {
        let (session, target_peer) = load_resumable_session(&self.db, session_id).await?;
        let files = crate::database::ops::get_session_files(&self.db, session_id).await?;

        // 重建 PreparedFile（不做 fs 探测，由 sender 端 chunk_request 失败时报错）
        let prepared_files = build_prepared_files_from_db(&files);
        let file_checksums = build_file_checksums(&files);
        let (resume_file_infos, _) = build_resume_file_infos(&files);

        let key = generate_key();

        info!(
            "发送方发起断点续传: session={}, files={}",
            session_id,
            file_checksums.len()
        );

        let resume_state = build_sender_resume_state(&files);

        let send_session = Arc::new(SendSession::new_with_resume(
            session_id,
            target_peer,
            prepared_files,
            &key,
            self.file_access.clone(),
            self.event_bus.clone(),
            &resume_state,
        ));
        self.send_sessions.insert(session_id, send_session);

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
                crate::database::ops::mark_session_transferring(&self.db, session_id).await?;
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
                info!("ResumeOffer rejected: {}", reason_str);
                crate::database::ops::mark_session_failed(&self.db, session_id, reason_str).await?;
                Err(AppError::Transfer(reason_str.into()))
            }
            other => {
                self.send_sessions.remove(&session_id);
                Err(AppError::Transfer(format!("意外的响应类型: {other:?}")))
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
        save_location: entity::SaveLocation,
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
    fn start_receive_session(
        &self,
        session_id: Uuid,
        peer_id: PeerId,
        files: Vec<FileInfo>,
        total_size: u64,
        save_location: entity::SaveLocation,
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

    async fn handle_complete(&self, session_id: Uuid) -> AppResult<TransferCompleteOutcome> {
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
        }

        Ok(TransferCompleteOutcome {
            event: crate::transfer::progress::TransferCompleteEvent {
                session_id,
                direction: TransferDirection::Send,
                total_bytes,
                elapsed_ms,
                save_location: None,
            },
            db_error,
        })
    }

    async fn handle_cancel(
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
            direction: TransferDirection::Unknown,
            error: format!("对方取消: {reason}"),
        })
    }

    async fn handle_pause(
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
            TransferDirection::Send
        } else if let Some(session) = self.get_receive_session(&session_id) {
            self.remove_receive_session(&session_id);
            session.cancel_and_wait().await;
            TransferDirection::Receive
        } else {
            TransferDirection::Unknown
        };

        if let Err(e) = crate::database::ops::mark_session_paused(&self.db, session_id).await {
            warn!("DB 标记暂停失败: {}", e);
        }
        if let Err(e) =
            crate::database::ops::sync_session_transferred_bytes(&self.db, session_id).await
        {
            warn!("同步 session 字节数失败: {}", e);
        }

        Ok(crate::transfer::progress::TransferPausedEvent {
            session_id,
            direction,
        })
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
        // 验证 session
        let ctx = match validate_resume_session(&self.db, session_id, &file_checksums).await {
            Ok(ctx) => ctx,
            Err(reason) => {
                return Ok(TransferResponse::ResumeResult {
                    session_id,
                    accepted: false,
                    reason: Some(reason),
                    key: None,
                });
            }
        };

        // 重建 PreparedFile（不做 fs 探测，让 chunk_request 失败时报错）
        let prepared_files = build_prepared_files_from_db(&ctx.db_files);
        let key = generate_key();
        let resume_state = build_sender_resume_state(&ctx.db_files);

        let send_session = Arc::new(SendSession::new_with_resume(
            session_id,
            peer_id,
            prepared_files,
            &key,
            self.file_access.clone(),
            self.event_bus.clone(),
            &resume_state,
        ));
        self.insert_send_session(session_id, send_session);

        Ok(TransferResponse::ResumeResult {
            session_id,
            accepted: true,
            reason: None,
            key: Some(key),
        })
    }

    async fn handle_resume_offer(
        &self,
        peer_id: PeerId,
        session_id: Uuid,
        key: [u8; 32],
        file_checksums: Vec<FileChecksum>,
    ) -> AppResult<TransferResponse> {
        let ctx = match validate_resume_session(&self.db, session_id, &file_checksums).await {
            Ok(ctx) => ctx,
            Err(reason) => {
                return Ok(TransferResponse::ResumeOfferResult {
                    session_id,
                    accepted: false,
                    reason: Some(reason),
                });
            }
        };

        let (file_infos, initial_bitmaps) = build_file_infos_and_bitmaps(&ctx.db_files);
        let save_location = ctx.session.save_path.unwrap_or(entity::SaveLocation::Path {
            path: String::new(),
        });
        let total_size = ctx.session.total_size as u64;
        let peer_name = ctx.session.peer_name.clone();
        let peer_id_str = ctx.session.peer_id.0.clone();

        // 构造 resumed 文件信息事件
        let resumed_files: Vec<crate::transfer::progress::TransferResumedFileInfo> = ctx
            .db_files
            .iter()
            .map(|f| crate::transfer::progress::TransferResumedFileInfo {
                file_id: f.file_id as u32,
                name: f.name.clone(),
                relative_path: f.relative_path.clone(),
                size: f.size as u64,
                is_directory: false,
            })
            .collect();

        self.start_receive_from_offer(
            session_id,
            peer_id,
            file_infos,
            total_size,
            save_location,
            &key,
            initial_bitmaps,
        );

        let _ = self
            .event_bus
            .publish(CoreEvent::TransferResumed {
                event: crate::transfer::progress::TransferResumedEvent {
                    session_id,
                    direction: TransferDirection::Receive,
                    peer_id: peer_id_str,
                    peer_name,
                    files: resumed_files,
                    total_size,
                },
            })
            .await;

        Ok(TransferResponse::ResumeOfferResult {
            session_id,
            accepted: true,
            reason: None,
        })
    }
}

/// 断点续传校验：DB 查询 → 状态检查 → 文件校验
struct ResumeContext {
    session: entity::transfer_session::Model,
    db_files: Vec<entity::transfer_file::Model>,
}

async fn validate_resume_session(
    db: &DatabaseConnection,
    session_id: Uuid,
    file_checksums: &[FileChecksum],
) -> Result<ResumeContext, ResumeRejectReason> {
    let session = entity::TransferSession::find_by_id(session_id)
        .one(db)
        .await
        .ok()
        .flatten()
        .ok_or(ResumeRejectReason::SessionNotFound)?;

    if session.status == entity::SessionStatus::Cancelled {
        return Err(ResumeRejectReason::SenderCancelled);
    }

    let db_files = crate::database::ops::get_session_files(db, session_id)
        .await
        .map_err(|_| ResumeRejectReason::SessionNotFound)?;

    for fc in file_checksums {
        let matched = db_files
            .iter()
            .any(|f| f.file_id == fc.file_id as i32 && f.checksum == fc.checksum);
        if !matched {
            return Err(ResumeRejectReason::FileModified);
        }
    }

    if let Err(e) = crate::database::ops::mark_session_transferring(db, session_id).await {
        warn!("DB 标记 session transferring 失败: {}", e);
    }

    Ok(ResumeContext { session, db_files })
}

// ============ 断点续传辅助函数 ============

pub(crate) fn build_sender_resume_state(
    files: &[entity::transfer_file::Model],
) -> HashMap<u32, (u32, u64)> {
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

            let chunks_done = if transferred >= file_size {
                total_chunks
            } else {
                (transferred.div_ceil(chunk_size)) as u32
            };

            Some((file_id, (chunks_done, transferred)))
        })
        .collect()
}

pub(crate) fn parse_peer_id(s: &str) -> AppResult<PeerId> {
    s.parse()
        .map_err(|_| AppError::Transfer(format!("无效的 PeerId: {s}")))
}

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

pub(crate) fn build_file_checksums(files: &[entity::transfer_file::Model]) -> Vec<FileChecksum> {
    files
        .iter()
        .map(|f| FileChecksum {
            file_id: f.file_id as u32,
            checksum: f.checksum.clone(),
        })
        .collect()
}

pub(crate) fn build_file_infos_and_bitmaps(
    files: &[entity::transfer_file::Model],
) -> (Vec<FileInfo>, HashMap<u32, Vec<u8>>) {
    let mut file_infos = Vec::with_capacity(files.len());
    let mut bitmaps = HashMap::with_capacity(files.len());
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

/// 从 DB 重建 PreparedFile，不做 fs 探测（让 sender 端 chunk_request 失败时报错）
pub(crate) fn build_prepared_files_from_db(
    files: &[entity::transfer_file::Model],
) -> Vec<PreparedFile> {
    files
        .iter()
        .map(|f| PreparedFile {
            file_id: f.file_id as u32,
            name: f.name.clone(),
            relative_path: f.relative_path.clone(),
            source_id: FileSourceId(f.source_path.clone().unwrap_or_default()),
            size: f.size as u64,
            checksum: f.checksum.clone(),
        })
        .collect()
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
