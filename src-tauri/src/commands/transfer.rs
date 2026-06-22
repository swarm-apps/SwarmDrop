//! 文件传输 Tauri 命令薄壳
//!
//! 业务逻辑全部在 [`swarmdrop_core::transfer::manager::TransferManager`]，
//! 这里只做：参数转换 + Tauri State / Channel 注册 + 调用 core。

use std::sync::Arc;

use sea_orm::EntityTrait;
use serde::Serialize;
use swarmdrop_core::transfer::manager::{StartSendResult, TransferManager};
use swarmdrop_core::transfer::progress::PrepareProgressEvent;
use swarmdrop_core::transfer::HostEnumeratedFile;
use tauri::ipc::Channel;
use tauri::{Manager, State};
use uuid::Uuid;

use crate::host::event_bus::PrepareChannelGuard;
use crate::host::file_source::{source_id, EnumeratedFile, FileSource};
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
    Ok(transfer.send_offer(&prepared_id, &peer_id, &peer_name, &selected_file_ids)?)
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
    db: State<'_, sea_orm::DatabaseConnection>,
    net: State<'_, NetManagerState>,
    session_id: Uuid,
) -> crate::AppResult<()> {
    let transfer = get_transfer(&net).await?;
    transfer.cancel_receive(&session_id).await?;
    crate::database::ops::mark_session_cancelled(&db, session_id).await?;
    Ok(())
}

// ============ 传输历史 API ============

#[tauri::command]
#[specta::specta]
pub async fn get_transfer_history(
    db: State<'_, sea_orm::DatabaseConnection>,
    status: Option<entity::SessionStatus>,
) -> crate::AppResult<Vec<crate::database::ops::TransferHistoryItem>> {
    Ok(crate::database::ops::get_transfer_history(&db, status).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn get_transfer_session(
    db: State<'_, sea_orm::DatabaseConnection>,
    session_id: Uuid,
) -> crate::AppResult<crate::database::ops::TransferHistoryItem> {
    Ok(crate::database::ops::get_session_detail(&db, session_id).await?)
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
    if transfer.pause_send(&session_id).await.is_err() {
        let _ = transfer.pause_receive(&session_id).await;
    }
    Ok(())
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

    let (resume_info, direction_str) = match session.direction {
        entity::TransferDirection::Receive => {
            (transfer.initiate_resume(session_id).await?, "receive")
        }
        entity::TransferDirection::Send => (
            transfer.initiate_resume_as_sender(session_id).await?,
            "send",
        ),
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

// ============ 辅助函数 ============

async fn get_transfer(net: &NetManagerState) -> crate::AppResult<Arc<TransferManager>> {
    let guard = net.lock().await;
    let manager = guard.as_ref().ok_or(crate::AppError::node_not_started())?;
    Ok(manager.transfer_arc())
}
