//! 文件传输 Tauri 命令薄壳
//!
//! 业务逻辑全部在 [`swarmdrop_core::transfer::manager::TransferManager`]，
//! 这里只做：参数转换 + Tauri State / Channel 注册 + 调用 core。

use std::sync::Arc;

use sea_orm::EntityTrait;
use serde::Serialize;
use swarmdrop_core::transfer::HostEnumeratedFile;
use swarmdrop_core::transfer::manager::{StartSendResult, TransferManager};
use swarmdrop_core::transfer::progress::PrepareProgressEvent;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

use crate::host::event_bus::PrepareChannelGuard;
use crate::host::file_source::{EnumeratedFile, FileSource, source_id};
use crate::network::NetManagerState;

// ============ scan_sources ============

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ScannedSourceResult {
    pub is_directory: bool,
    pub files: Vec<EnumeratedFile>,
    pub total_size: u64,
}

#[tauri::command]
#[specta::specta]
pub async fn scan_sources(
    app: tauri::AppHandle,
    sources: Vec<FileSource>,
) -> crate::AppResult<Vec<ScannedSourceResult>> {
    let mut results = Vec::new();
    for source in sources {
        let meta = source.metadata(&app).await?;
        if meta.is_dir {
            let entries = source.enumerate_dir(&meta.name, &app).await?;
            let total_size: u64 = entries.iter().map(|e| e.size).sum();
            results.push(ScannedSourceResult {
                is_directory: true,
                files: entries,
                total_size,
            });
        } else {
            results.push(ScannedSourceResult {
                is_directory: false,
                total_size: meta.size,
                files: vec![EnumeratedFile {
                    name: meta.name.clone(),
                    relative_path: meta.name,
                    source,
                    size: meta.size,
                }],
            });
        }
    }
    Ok(results)
}

// ============ prepare_send ============

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TransferFileResult {
    pub file_id: u32,
    pub name: String,
    pub relative_path: String,
    pub size: u64,
    pub is_directory: bool,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PreparedTransferResult {
    pub prepared_id: Uuid,
    pub files: Vec<TransferFileResult>,
    pub total_size: u64,
}

#[tauri::command]
#[specta::specta]
pub async fn prepare_send(
    app: tauri::AppHandle,
    net: State<'_, NetManagerState>,
    files: Vec<EnumeratedFile>,
    on_progress: Channel<PrepareProgressEvent>,
) -> crate::AppResult<PreparedTransferResult> {
    let transfer = get_transfer(&net).await?;
    let prepared_id = Uuid::new_v4();

    // RAII guard：无论 prepare 成功/失败/panic，drop 时自动 unregister channel
    let event_bus = app.state::<crate::host::event_bus::TauriEventBus>();
    let _channel_guard = PrepareChannelGuard::register(&event_bus, prepared_id, on_progress);

    let host_files: Vec<HostEnumeratedFile> = files
        .into_iter()
        .map(|f| HostEnumeratedFile {
            source_id: source_id(&f.source),
            name: f.name,
            relative_path: f.relative_path,
            size: f.size,
        })
        .collect();

    let prepared = transfer.prepare(prepared_id, host_files).await?;
    Ok(PreparedTransferResult {
        prepared_id: prepared.prepared_id,
        total_size: prepared.total_size,
        files: prepared
            .files
            .iter()
            .map(|f| TransferFileResult {
                file_id: f.file_id,
                name: f.name.clone(),
                relative_path: f.relative_path.clone(),
                size: f.size,
                is_directory: false,
            })
            .collect(),
    })
}

#[tauri::command]
#[specta::specta]
pub async fn start_send(
    net: State<'_, NetManagerState>,
    prepared_id: Uuid,
    peer_id: String,
    peer_name: String,
    selected_file_ids: Vec<u32>,
) -> crate::AppResult<StartSendResult> {
    let transfer = get_transfer(&net).await?;
    Ok(transfer
        .send_offer(
            &prepared_id,
            &peer_id,
            &peer_name,
            &selected_file_ids,
            swarmdrop_core::protocol::TransferOrigin::Human,
        )
        .await?)
}

#[tauri::command]
#[specta::specta]
pub async fn accept_receive(
    net: State<'_, NetManagerState>,
    session_id: Uuid,
    save_location: swarmdrop_core::host::CoreSaveLocation,
) -> crate::AppResult<()> {
    let transfer = get_transfer(&net).await?;
    Ok(transfer
        .accept_and_start_receive(&session_id, save_location)
        .await?)
}

#[tauri::command]
#[specta::specta]
pub async fn reject_receive(
    net: State<'_, NetManagerState>,
    session_id: Uuid,
) -> crate::AppResult<()> {
    let transfer = get_transfer(&net).await?;
    Ok(transfer.reject_and_respond(&session_id).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn cancel_send(
    net: State<'_, NetManagerState>,
    session_id: Uuid,
) -> crate::AppResult<()> {
    let transfer = get_transfer(&net).await?;
    Ok(transfer.cancel_send(&session_id).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn cancel_receive(
    net: State<'_, NetManagerState>,
    session_id: Uuid,
) -> crate::AppResult<()> {
    let transfer = get_transfer(&net).await?;
    Ok(transfer.cancel_receive(&session_id).await?)
}

// ============ 传输投影 API ============

#[tauri::command]
#[specta::specta]
pub async fn get_transfer_projections(
    db: State<'_, sea_orm::DatabaseConnection>,
) -> crate::AppResult<Vec<crate::database::ops::TransferProjection>> {
    Ok(crate::database::ops::get_transfer_projections(&db).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn delete_transfer_session(
    db: State<'_, sea_orm::DatabaseConnection>,
    session_id: Uuid,
) -> crate::AppResult<()> {
    Ok(crate::database::ops::delete_session(&db, session_id).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn clear_transfer_history(
    db: State<'_, sea_orm::DatabaseConnection>,
) -> crate::AppResult<()> {
    Ok(crate::database::ops::clear_all_history(&db).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn pause_transfer(
    net: State<'_, NetManagerState>,
    session_id: Uuid,
) -> crate::AppResult<()> {
    let transfer = get_transfer(&net).await?;
    match transfer.pause_send(&session_id).await {
        Ok(()) => Ok(()),
        Err(send_err) => match transfer.pause_receive(&session_id).await {
            Ok(()) => Ok(()),
            Err(receive_err) => Err(crate::AppError::transfer(format!(
                "暂停传输失败: {send_err}; {receive_err}"
            ))),
        },
    }
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ResumeTransferResult {
    pub session_id: Uuid,
    pub direction: String,
    pub peer_id: String,
    pub peer_name: String,
    pub files: Vec<TransferFileResult>,
    pub total_size: u64,
    pub transferred_bytes: u64,
}

#[tauri::command]
#[specta::specta]
pub async fn resume_transfer(
    db: State<'_, sea_orm::DatabaseConnection>,
    net: State<'_, NetManagerState>,
    session_id: Uuid,
) -> crate::AppResult<ResumeTransferResult> {
    let transfer = get_transfer(&net).await?;

    let session = entity::TransferSession::find_by_id(session_id)
        .one(db.inner())
        .await?
        .ok_or_else(|| crate::AppError::transfer("会话不存在"))?;

    // 发送方/接收方发起恢复已统一为单入口 `initiate_resume`（内部按 session.direction 派生）。
    let resume_info = transfer.initiate_resume(session_id).await?;
    let direction_str = match session.direction {
        entity::TransferDirection::Receive => "receive",
        entity::TransferDirection::Send => "send",
    };

    Ok(ResumeTransferResult {
        session_id,
        direction: direction_str.into(),
        peer_id: resume_info.peer_id,
        peer_name: resume_info.peer_name,
        files: resume_info
            .files
            .iter()
            .map(|f| TransferFileResult {
                file_id: f.file_id as u32,
                name: f.name.clone(),
                relative_path: f.relative_path.clone(),
                size: f.size as u64,
                is_directory: false,
            })
            .collect(),
        total_size: resume_info.total_size as u64,
        transferred_bytes: resume_info.transferred_bytes as u64,
    })
}

// ============ 暂停接收 ============

/// 设置全局「暂停接收」。`true`=暂停：节点保持在线可发现、配对不受影响，但对新 offer
/// 自动婉拒；`false`=恢复：新 offer 照常按既有策略处理。同步托盘文案并广播
/// `receiving-paused-changed` 事件。
#[tauri::command]
#[specta::specta]
pub async fn set_receiving_paused(app: AppHandle, paused: bool) -> crate::AppResult<()> {
    crate::tray::apply_receiving_paused(&app, paused).await
}

/// 查询当前是否暂停接收（节点未启动视为未暂停）。
#[tauri::command]
#[specta::specta]
pub async fn is_receiving_paused(app: AppHandle) -> crate::AppResult<bool> {
    Ok(crate::tray::current_receiving_paused(&app).await)
}

// ============ 辅助函数 ============

async fn get_transfer(net: &NetManagerState) -> crate::AppResult<Arc<TransferManager>> {
    let guard = net.lock().await;
    let manager = guard.as_ref().ok_or(crate::AppError::node_not_started())?;
    Ok(manager.transfer_arc())
}
