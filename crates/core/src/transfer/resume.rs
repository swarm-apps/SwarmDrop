//! 断点续传：发送方/接收方双侧发起 + IncomingTransferRuntime 续传 helper。
//!
//! 私有 `validate_resume_session` + `ResumeContext` 集中在这里；
//! 通用辅助函数（build_*）也下沉到本文件，因为它们仅服务于 resume 流程。

use std::collections::HashMap;
use std::sync::Arc;

use sea_orm::{DatabaseConnection, EntityTrait};
use swarm_p2p_core::libp2p::PeerId;
use tracing::{info, warn};
use uuid::Uuid;

use crate::host::{CoreEvent, FileSourceId};
use crate::protocol::{
    AppRequest, AppResponse, FileChecksum, FileInfo, ResumeRejectReason, TransferRequest,
    TransferResponse,
};
use crate::transfer::crypto::generate_key;
use crate::transfer::manager::{
    PreparedFile, ResumeFileInfo, ResumeInfo, TransferManager,
};
use crate::transfer::progress::TransferDirection;
use crate::transfer::sender::SendSession;
use crate::transfer::{calc_total_chunks, CHUNK_SIZE};
use crate::{AppError, AppResult};

impl TransferManager {
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
                let save_location = session
                    .save_path
                    .map(crate::host::CoreSaveLocation::from)
                    .unwrap_or(crate::host::CoreSaveLocation::Path {
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
}

// ============ IncomingTransferRuntime 续传侧 helper（被 manager.rs 中 trait impl 调用） ============

impl TransferManager {
    pub(super) async fn handle_resume_request_impl(
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

    pub(super) async fn handle_resume_offer_impl(
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
        let save_location = ctx
            .session
            .save_path
            .map(crate::host::CoreSaveLocation::from)
            .unwrap_or(crate::host::CoreSaveLocation::Path {
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

// ============ 续传共用的私有 helper ============

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
