use serde::Serialize;
use swarm_p2p_core::{EventReceiver, NodeEvent};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;
use tracing::{info, warn};
use uuid::Uuid;

use sea_orm::DatabaseConnection;

use super::manager::SharedNetRefs;
use crate::device::DeviceFilter;
use crate::events;
use crate::protocol::{
    AppNetClient, AppRequest, AppResponse, OfferRejectReason, PairingRequest, ResumeRejectReason,
    TransferRequest, TransferResponse,
};
use crate::transfer::progress::{TransferDbErrorEvent, TransferDirection, TransferFailedEvent, TransferPausedEvent, TransferResumedEvent, TransferResumedFileInfo};
use swarm_p2p_core::libp2p::PeerId;

/// 配对请求事件 payload
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PairingRequestPayload {
    peer_id: PeerId,
    pending_id: u64,
    #[serde(flatten)]
    request: PairingRequest,
}

/// 传输 Offer 事件 payload（推送给前端）
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct TransferOfferPayload {
    session_id: Uuid,
    peer_id: String,
    device_name: String,
    files: Vec<TransferFilePayload>,
    total_size: u64,
}

/// Offer 中的文件信息（前端展示用）
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct TransferFilePayload {
    file_id: u32,
    name: String,
    relative_path: String,
    size: u64,
    is_directory: bool,
}

use std::path::PathBuf;
use std::sync::Arc;

use sea_orm::EntityTrait;

use crate::file_source::FileSource;
use crate::protocol::FileChecksum;
use crate::transfer::offer::{
    build_file_infos_and_bitmaps, build_sender_resume_state, PreparedFile, TransferManager,
};
use crate::transfer::sender::SendSession;

/// 断点续传的共用验证结果
struct ResumeValidation {
    session: entity::transfer_session::Model,
    db_files: Vec<entity::transfer_file::Model>,
    db: DatabaseConnection,
}

/// 校验断点续传的前置条件：DB 连接、session 存在且未取消、文件 checksum 匹配
async fn validate_resume(
    app: &AppHandle,
    session_id: Uuid,
    file_checksums: &[FileChecksum],
) -> Result<ResumeValidation, ResumeRejectReason> {
    let db = app
        .try_state::<DatabaseConnection>()
        .map(|db| db.inner().clone())
        .ok_or(ResumeRejectReason::SessionNotFound)?;

    let session = entity::TransferSession::find_by_id(session_id)
        .one(&db)
        .await
        .ok()
        .flatten()
        .ok_or(ResumeRejectReason::SessionNotFound)?;

    if session.status == entity::SessionStatus::Cancelled {
        return Err(ResumeRejectReason::SenderCancelled);
    }

    let db_files = crate::database::ops::get_session_files(&db, session_id)
        .await
        .map_err(|_| ResumeRejectReason::SessionNotFound)?;

    // 校验文件 checksum 是否匹配
    let all_matched = file_checksums.iter().all(|fc| {
        db_files
            .iter()
            .any(|f| f.file_id == fc.file_id as i32 && f.checksum == fc.checksum)
    });
    if !all_matched {
        return Err(ResumeRejectReason::FileModified);
    }

    Ok(ResumeValidation {
        session,
        db_files,
        db,
    })
}

/// 断点续传校验上下文（公共校验阶段的输出）
struct ResumeContext {
    session: entity::transfer_session::Model,
    db_files: Vec<entity::transfer_file::Model>,
}

/// 断点续传公共校验：获取 DB → 查 session → 检查状态 → 获取文件 → 校验 checksum
///
/// 成功返回 `ResumeContext`，失败返回预构造的拒绝响应（由调用方包装为具体的 Response 类型）。
async fn validate_resume_session(
    app: &AppHandle,
    session_id: Uuid,
    file_checksums: &[FileChecksum],
) -> Result<ResumeContext, ResumeRejectReason> {
    let db = app
        .try_state::<DatabaseConnection>()
        .map(|s| s.inner().clone())
        .ok_or(ResumeRejectReason::SessionNotFound)?;

    let session = entity::TransferSession::find_by_id(session_id)
        .one(&db)
        .await
        .ok()
        .flatten()
        .ok_or(ResumeRejectReason::SessionNotFound)?;

    if session.status == entity::SessionStatus::Cancelled {
        return Err(ResumeRejectReason::SenderCancelled);
    }

    let db_files = crate::database::ops::get_session_files(&db, session_id)
        .await
        .map_err(|_| ResumeRejectReason::SessionNotFound)?;

    // 校验文件 checksum 是否匹配
    for fc in file_checksums {
        let matched = db_files
            .iter()
            .any(|f| f.file_id == fc.file_id as i32 && f.checksum == fc.checksum);
        if !matched {
            return Err(ResumeRejectReason::FileModified);
        }
    }

    // 更新 DB 状态
    if let Err(e) = crate::database::ops::mark_session_transferring(&db, session_id).await {
        warn!("DB 标记 session transferring 失败: {}", e);
    }

    Ok(ResumeContext {
        session,
        db_files,
    })
}

/// 发送方处理 ResumeRequest：验证文件校验和，重建 PreparedFile，创建 SendSession
async fn handle_resume_request(
    app: &AppHandle,
    session_id: Uuid,
    peer_id: PeerId,
    file_checksums: &[FileChecksum],
    transfer: &Arc<TransferManager>,
) -> TransferResponse {
    let ctx = match validate_resume_session(app, session_id, file_checksums).await {
        Ok(ctx) => ctx,
        Err(reason) => return reject_resume(session_id, reason),
    };

    // 从 DB 文件记录重建 PreparedFile 列表
    let mut prepared_files = Vec::with_capacity(ctx.db_files.len());
    for db_file in &ctx.db_files {
        let Some(source_path) = &db_file.source_path else {
            warn!("文件缺少 source_path: file_id={}", db_file.file_id);
            return TransferResponse::ResumeResult {
                session_id,
                accepted: false,
                reason: Some(ResumeRejectReason::SessionNotFound),
                key: None,
            };
        };

        // 验证源文件仍存在且大小匹配
        let path = PathBuf::from(source_path);
        match tokio::fs::metadata(&path).await {
            Ok(meta) if meta.len() == db_file.size as u64 => {}
            _ => {
                warn!("源文件不存在或大小不匹配: {}", source_path);
                return TransferResponse::ResumeResult {
                    session_id,
                    accepted: false,
                    reason: Some(ResumeRejectReason::FileModified),
                    key: None,
                };
            }
        }

        prepared_files.push(PreparedFile {
            file_id: db_file.file_id as u32,
            name: db_file.name.clone(),
            relative_path: db_file.relative_path.clone(),
            source: FileSource::Path { path },
            size: db_file.size as u64,
            checksum: db_file.checksum.clone(),
        });
    }

    // 生成密钥（发送方不持久化密钥，每次恢复重新生成）
    let key = crate::transfer::crypto::generate_key();

    // 从 DB 构建 resume_state（file_id → (chunks_done, transferred_bytes)）
    let resume_state = build_sender_resume_state(&ctx.db_files);

    // 创建 SendSession 并注册到 TransferManager（带 resume 状态）
    let send_session = Arc::new(SendSession::new_with_resume(
        session_id,
        peer_id,
        prepared_files,
        &key,
        app.clone(),
        &resume_state,
    ));
    transfer.insert_send_session(session_id, send_session);

    info!("接受断点续传: session={}", session_id);

    TransferResponse::ResumeResult {
        session_id,
        accepted: true,
        reason: None,
        key: Some(key),
    }
}

/// 接收方处理 ResumeOffer：验证文件校验和，创建 ReceiveSession，开始拉取
async fn handle_resume_offer(
    app: &AppHandle,
    session_id: Uuid,
    peer_id: PeerId,
    key: &[u8; 32],
    file_checksums: &[FileChecksum],
    transfer: &Arc<TransferManager>,
) -> TransferResponse {
    let ctx = match validate_resume_session(app, session_id, file_checksums).await {
        Ok(ctx) => ctx,
        Err(reason) => return reject_resume_offer(session_id, reason),
    };

    // 构建 FileInfo、initial_bitmaps（复用已有辅助函数）
    let (file_infos, initial_bitmaps) = build_file_infos_and_bitmaps(&ctx.db_files);

    let mut resumed_file_infos = Vec::with_capacity(ctx.db_files.len());
    for f in &ctx.db_files {
        resumed_file_infos.push(TransferResumedFileInfo {
            file_id: f.file_id as u32,
            name: f.name.clone(),
            relative_path: f.relative_path.clone(),
            size: f.size as u64,
            is_directory: false,
        });
    }

    let save_location = ctx.session.save_path.unwrap_or(entity::SaveLocation::Path {
        path: String::new(),
    });
    let total_size = ctx.session.total_size as u64;
    let peer_name = ctx.session.peer_name.clone();
    let peer_id_str = ctx.session.peer_id.0.clone();

    // 创建 ReceiveSession 并开始拉取
    transfer.start_receive_from_offer(
        session_id,
        peer_id,
        file_infos,
        total_size,
        crate::transfer::offer::build_file_sink(&save_location),
        key,
        app.clone(),
        initial_bitmaps,
    );

    // 发射 TRANSFER_RESUMED 事件给前端
    let _ = app.emit(
        events::TRANSFER_RESUMED,
        TransferResumedEvent {
            session_id,
            direction: TransferDirection::Receive,
            peer_id: peer_id_str,
            peer_name,
            files: resumed_file_infos,
            total_size,
        },
    );

    info!("接受发送方断点续传: session={}", session_id);

    TransferResponse::ResumeOfferResult {
        session_id,
        accepted: true,
        reason: None,
    }
}

/// 发送 Ack 响应并执行 DB 操作（合并为一个异步任务）
fn spawn_ack_and_db<F, Fut>(
    client: AppNetClient,
    app: AppHandle,
    pending_id: u64,
    session_id: Uuid,
    db_op: F,
) where
    F: FnOnce(DatabaseConnection, Uuid) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send,
{
    tokio::spawn(async move {
        let response = AppResponse::Transfer(TransferResponse::Ack { session_id });
        let _ = client.send_response(pending_id, response).await;
        if let Some(db) = app.try_state::<DatabaseConnection>() {
            db_op(db.inner().clone(), session_id).await;
        }
    });
}

/// 保存发送方 per-file 进度到 DB（断点续传恢复时使用）
pub(crate) async fn save_sender_file_progress(
    app: &AppHandle,
    session_id: Uuid,
    session: &SendSession,
) {
    if let Some(db) = app.try_state::<DatabaseConnection>() {
        for (file_id, _chunks_done, transferred) in session.get_file_progress() {
            if transferred > 0 {
                if let Err(e) = crate::database::ops::update_sender_file_progress(
                    &db,
                    session_id,
                    file_id as i32,
                    transferred as i64,
                )
                .await
                {
                    warn!("保存发送方文件进度失败: file_id={}, {}", file_id, e);
                }
            }
        }
    }
}

/// 当窗口未聚焦时发送系统通知
fn notify_if_unfocused(app: &AppHandle, title: &str, body: &str) {
    let focused = app
        .get_webview_window("main")
        .and_then(|w| w.is_focused().ok())
        .unwrap_or(false);

    if !focused {
        if let Err(e) = app.notification().builder().title(title).body(body).show() {
            warn!("发送通知失败: {}", e);
        }
    }
}

/// 启动事件循环：后端消费所有 NodeEvent，通过 Tauri Event 推送高层域事件 + payload
///
/// 参照 libs/core 的责任链模式——前端不接触原始 NodeEvent。
pub fn spawn_event_loop(
    mut receiver: EventReceiver<AppRequest>,
    app: AppHandle,
    shared: SharedNetRefs,
) {
    tokio::spawn(async move {
        let emit_device_and_status = || {
            let devices = shared.devices.get_devices(DeviceFilter::All);
            let _ = app.emit(events::DEVICES_CHANGED, &devices);
            let net_status = shared.build_network_status();
            let _ = app.emit(events::NETWORK_STATUS_CHANGED, &net_status);
        };

        while let Some(event) = receiver.recv().await {
            // handle_event 对不相关的事件直接忽略，无条件调用后再消费 event
            shared.devices.handle_event(&event);

            match event {
                // === 网络状态事件 ===
                NodeEvent::Listening { addr } => {
                    if let Ok(mut addrs) = shared.listen_addrs.write() {
                        addrs.push(addr);
                    }
                    let status = shared.build_network_status();
                    let _ = app.emit(events::NETWORK_STATUS_CHANGED, &status);
                }
                NodeEvent::NatStatusChanged {
                    status,
                    public_addr,
                } => {
                    if let Ok(mut ns) = shared.nat_status.write() {
                        *ns = status;
                    }
                    if let Ok(mut pa) = shared.public_addr.write() {
                        *pa = public_addr;
                    }
                    let net_status = shared.build_network_status();
                    let _ = app.emit(events::NETWORK_STATUS_CHANGED, &net_status);
                }
                NodeEvent::RelayReservationAccepted { relay_peer_id, .. } => {
                    if let Ok(mut rp) = shared.relay_peers.write() {
                        rp.insert(relay_peer_id);
                    }
                    let net_status = shared.build_network_status();
                    let _ = app.emit(events::NETWORK_STATUS_CHANGED, &net_status);
                }

                // === 设备事件（handle_event 已在上方处理） ===
                NodeEvent::PeerConnected { .. } => {
                    emit_device_and_status();
                }
                NodeEvent::PeerDisconnected { ref peer_id } => {
                    // 清理中继节点
                    if let Ok(mut rp) = shared.relay_peers.write() {
                        rp.remove(peer_id);
                    }
                    emit_device_and_status();
                }
                NodeEvent::IdentifyReceived { .. }
                | NodeEvent::PeersDiscovered { .. }
                | NodeEvent::PingSuccess { .. }
                | NodeEvent::HolePunchSucceeded { .. } => {
                    emit_device_and_status();
                }

                // === 入站请求（缓存上下文 + 推送业务事件给前端） ===
                NodeEvent::InboundRequest {
                    peer_id,
                    pending_id,
                    request,
                } => {
                    info!("Inbound request from {:?}: {:?}", peer_id, request);

                    match request {
                        AppRequest::Pairing(req) => {
                            shared
                                .pairing
                                .cache_inbound_request(peer_id, pending_id, &req);
                            notify_if_unfocused(
                                &app,
                                "配对请求",
                                &format!("{} 请求与您配对", req.os_info.hostname),
                            );

                            let payload = PairingRequestPayload {
                                peer_id,
                                pending_id,
                                request: req,
                            };
                            let _ = app.emit(events::PAIRING_REQUEST_RECEIVED, &payload);
                        }

                        // === 分块传输请求（ChunkRequest / Complete / Cancel） ===
                        AppRequest::Transfer(TransferRequest::ChunkRequest {
                            session_id,
                            file_id,
                            chunk_index,
                        }) => {
                            let session = shared.transfer.get_send_session(&session_id);
                            let client = shared.client.clone();

                            tokio::spawn(async move {
                                let response = match session {
                                    Some(s) => {
                                        match s.handle_chunk_request(file_id, chunk_index).await {
                                            Ok(resp) => AppResponse::Transfer(resp),
                                            Err(e) => {
                                                warn!("ChunkRequest 处理失败: {}", e);
                                                AppResponse::Transfer(TransferResponse::ChunkError {
                                                    session_id,
                                                    file_id,
                                                    chunk_index,
                                                    error: e.to_string(),
                                                })
                                            }
                                        }
                                    }
                                    None => {
                                        warn!("未知的发送会话: {}", session_id);
                                        AppResponse::Transfer(TransferResponse::ChunkError {
                                            session_id,
                                            file_id,
                                            chunk_index,
                                            error: "发送会话不存在".into(),
                                        })
                                    }
                                };
                                if let Err(e) = client.send_response(pending_id, response).await {
                                    warn!("发送 Chunk 响应失败: {}", e);
                                }
                            });
                        }

                        AppRequest::Transfer(TransferRequest::Complete { session_id }) => {
                            // 获取统计数据后清理会话
                            let (total_bytes, elapsed_ms) = shared
                                .transfer
                                .get_send_session(&session_id)
                                .map(|s| {
                                    s.handle_complete();
                                    (s.total_bytes_sent(), s.elapsed_ms())
                                })
                                .unwrap_or((0, 0));
                            shared.transfer.remove_send_session(&session_id);

                            let client = shared.client.clone();
                            let app2 = app.clone();
                            tokio::spawn(async move {
                                let response =
                                    AppResponse::Transfer(TransferResponse::Ack { session_id });
                                if let Err(e) = client.send_response(pending_id, response).await {
                                    warn!("发送 Ack 响应失败: {}", e);
                                }

                                // DB: 标记发送方会话完成
                                if let Some(db) = app2.try_state::<DatabaseConnection>() {
                                    if let Err(e) = crate::database::ops::mark_session_completed(
                                        &db, session_id,
                                    )
                                    .await
                                    {
                                        warn!("DB 标记发送完成失败: {}", e);
                                        let _ = app2.emit(
                                            events::TRANSFER_DB_ERROR,
                                            TransferDbErrorEvent {
                                                session_id,
                                                message: format!("保存完成状态失败: {e}"),
                                            },
                                        );
                                    }
                                }

                                // 发送方也发射完成事件
                                let event = crate::transfer::progress::TransferCompleteEvent {
                                    session_id,
                                    direction: TransferDirection::Send,
                                    total_bytes,
                                    elapsed_ms,
                                    save_location: None,
                                };
                                let _ = app2.emit(events::TRANSFER_COMPLETE, &event);
                            });
                        }

                        AppRequest::Transfer(TransferRequest::Cancel { session_id, reason }) => {
                            info!(
                                "收到对方取消传输: session={}, reason={}",
                                session_id, reason
                            );

                            // 检查是否有发送会话
                            if let Some(s) = shared.transfer.get_send_session(&session_id) {
                                s.handle_cancel();
                                shared.transfer.remove_send_session(&session_id);
                            }

                            // 检查是否有接收会话
                            if let Some(s) = shared.transfer.get_receive_session(&session_id) {
                                shared.transfer.remove_receive_session(&session_id);
                                tokio::spawn(async move {
                                    s.cancel_and_wait().await;
                                    s.cleanup_part_files().await;
                                });
                            }

                            spawn_ack_and_db(
                                shared.client.clone(),
                                app.clone(),
                                pending_id,
                                session_id,
                                |db, sid| async move {
                                    if let Err(e) = crate::database::ops::mark_session_cancelled(&db, sid).await {
                                        warn!("DB 标记取消失败: {}", e);
                                    }
                                },
                            );

                            let _ = app.emit(
                                events::TRANSFER_FAILED,
                                TransferFailedEvent {
                                    session_id,
                                    direction: TransferDirection::Unknown,
                                    error: format!("对方取消: {}", reason),
                                },
                            );
                        }

                        AppRequest::Transfer(TransferRequest::Pause { session_id }) => {
                            info!("收到对方暂停传输: session={}", session_id);

                            // 检查是否有发送会话（对端是接收方暂停的）
                            let direction = if let Some(s) = shared.transfer.get_send_session(&session_id) {
                                // 保存发送方 per-file 进度到 DB（断点续传恢复时使用）
                                if let Some(db) = app.try_state::<DatabaseConnection>() {
                                    let progress = s.get_file_progress();
                                    let _ = crate::database::ops::save_sender_file_progress(
                                        &db, session_id, &progress,
                                    ).await;
                                }
                                s.cancel();
                                shared.transfer.remove_send_session(&session_id);
                                TransferDirection::Send
                            }
                            // 检查是否有接收会话（对端是发送方暂停的）
                            else if let Some(s) = shared.transfer.get_receive_session(&session_id) {
                                shared.transfer.remove_receive_session(&session_id);
                                s.cancel_and_wait().await;
                                TransferDirection::Receive
                            } else {
                                TransferDirection::Unknown
                            };

                            spawn_ack_and_db(
                                shared.client.clone(),
                                app.clone(),
                                pending_id,
                                session_id,
                                |db, sid| async move {
                                    if let Err(e) = crate::database::ops::mark_session_paused(&db, sid).await {
                                        warn!("DB 标记暂停失败: {}", e);
                                    }
                                    if let Err(e) = crate::database::ops::sync_session_transferred_bytes(&db, sid).await {
                                        warn!("同步 session 字节数失败: {}", e);
                                    }
                                },
                            );

                            let _ = app.emit(events::TRANSFER_PAUSED, TransferPausedEvent { session_id, direction });
                        }

                        AppRequest::Transfer(TransferRequest::Offer {
                            session_id,
                            files,
                            total_size,
                        }) => {
                            // 仅接受已配对设备的 Offer
                            if !shared.pairing.is_paired(&peer_id) {
                                warn!("Rejecting transfer offer from unpaired peer: {}", peer_id);
                                let response =
                                    AppResponse::Transfer(TransferResponse::OfferResult {
                                        accepted: false,
                                        key: None,
                                        reason: Some(OfferRejectReason::NotPaired),
                                    });
                                let client = shared.client.clone();
                                tokio::spawn(async move {
                                    if let Err(e) = client.send_response(pending_id, response).await
                                    {
                                        warn!("Failed to reject offer: {}", e);
                                    }
                                });
                                continue;
                            }

                            // 获取设备名
                            let device_name = shared
                                .pairing
                                .get_paired_devices()
                                .into_iter()
                                .find(|d| d.peer_id == peer_id)
                                .map(|d| d.os_info.hostname)
                                .unwrap_or_else(|| {
                                    let s = peer_id.to_string();
                                    s[s.len().saturating_sub(8)..].to_string()
                                });

                            // 缓存入站 Offer
                            shared.transfer.cache_inbound_offer(
                                pending_id,
                                peer_id,
                                device_name.clone(),
                                session_id,
                                files.clone(),
                                total_size,
                            );

                            // 通知前端
                            let payload = TransferOfferPayload {
                                session_id,
                                peer_id: peer_id.to_string(),
                                device_name: device_name.clone(),
                                files: files
                                    .into_iter()
                                    .map(|f| TransferFilePayload {
                                        file_id: f.file_id,
                                        name: f.name,
                                        relative_path: f.relative_path,
                                        size: f.size,
                                        is_directory: false,
                                    })
                                    .collect(),
                                total_size,
                            };
                            let _ = app.emit(events::TRANSFER_OFFER, &payload);

                            notify_if_unfocused(
                                &app,
                                "收到文件传输请求",
                                &format!("{} 想要向您发送文件", device_name),
                            );
                        }

                        // === 断点续传请求（发送方处理接收方的 ResumeRequest） ===
                        AppRequest::Transfer(TransferRequest::ResumeRequest {
                            session_id,
                            file_checksums,
                        }) => {
                            info!(
                                "收到断点续传请求: session={}, files={}",
                                session_id,
                                file_checksums.len()
                            );

                            let client = shared.client.clone();
                            let app2 = app.clone();
                            let transfer = shared.transfer.clone();

                            tokio::spawn(async move {
                                let response = handle_resume_request(
                                    &app2,
                                    session_id,
                                    peer_id,
                                    &file_checksums,
                                    &transfer,
                                )
                                .await;
                                if let Err(e) = client
                                    .send_response(pending_id, AppResponse::Transfer(response))
                                    .await
                                {
                                    warn!("发送 ResumeResult 失败: {}", e);
                                }
                            });
                        }

                        // === 发送方发起的断点续传（接收方处理 ResumeOffer） ===
                        AppRequest::Transfer(TransferRequest::ResumeOffer {
                            session_id,
                            key,
                            file_checksums,
                        }) => {
                            info!(
                                "收到发送方断点续传请求: session={}, files={}",
                                session_id,
                                file_checksums.len()
                            );

                            let client = shared.client.clone();
                            let app2 = app.clone();
                            let transfer = shared.transfer.clone();

                            tokio::spawn(async move {
                                let response = handle_resume_offer(
                                    &app2,
                                    session_id,
                                    peer_id,
                                    &key,
                                    &file_checksums,
                                    &transfer,
                                )
                                .await;
                                if let Err(e) = client
                                    .send_response(pending_id, AppResponse::Transfer(response))
                                    .await
                                {
                                    warn!("发送 ResumeOfferResult 失败: {}", e);
                                }
                            });
                        }
                    }
                }

                // === 信息事件（仅日志，打洞失败仍走 Relay，无需通知用户） ===
                NodeEvent::HolePunchFailed { peer_id, error } => {
                    warn!("Hole punch failed with {}: {}", peer_id, error);
                }
            }
        }
    });
}
