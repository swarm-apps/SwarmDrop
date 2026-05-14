//! 文件传输相关 Tauri 命令
//!
//! 薄层命令入口，所有业务逻辑委托给 [`transfer`](crate::transfer) 模块。

use std::sync::Arc;

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::State;
use uuid::Uuid;

use crate::file_source::{EnumeratedFile, FileSource};
use crate::network::NetManagerState;
use crate::transfer::offer::{PrepareProgress, StartSendResult, TransferManager};
use sea_orm::EntityTrait;

// ============ scan_sources ============

/// scan_sources 返回的单个来源扫描结果
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScannedSourceResult {
    /// 是否为目录
    pub is_directory: bool,
    /// 包含的文件列表（每个文件带有 source 用于后续传给 prepare_send）
    pub files: Vec<EnumeratedFile>,
    /// 此来源的总大小
    pub total_size: u64,
}

/// 扫描文件来源：遍历目录、收集元数据，不计算 hash
///
/// 用于用户选择文件/文件夹后在 UI 上展示文件树。
/// 每个 FileSource 返回一个 ScannedSourceResult，包含扁平化的文件列表。
#[tauri::command]
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

/// 准备好的文件信息（返回给前端）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferFileResult {
    pub file_id: u32,
    pub name: String,
    pub relative_path: String,
    pub size: u64,
    pub is_directory: bool,
}

/// prepare_send 的返回类型
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedTransferResult {
    pub prepared_id: Uuid,
    pub files: Vec<TransferFileResult>,
    pub total_size: u64,
}

/// 准备发送：对预扫描的文件列表计算 BLAKE3 校验和、分配 fileId
///
/// 接收 `scan_sources` 返回的 `EnumeratedFile` 列表（前端可能已过滤掉用户移除的文件）。
/// 不再做目录遍历，只计算 hash。通过 `on_progress` Channel 实时上报进度。
#[tauri::command]
pub async fn prepare_send(
    app: tauri::AppHandle,
    net: State<'_, NetManagerState>,
    files: Vec<EnumeratedFile>,
    on_progress: Channel<PrepareProgress>,
) -> crate::AppResult<PreparedTransferResult> {
    let transfer = get_transfer(&net).await?;
    let prepared = transfer.prepare(files, &app, on_progress).await?;

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

/// 开始发送：构造 Offer，发送到目标 peer（非阻塞，通过事件通知结果）
#[tauri::command]
pub async fn start_send(
    app: tauri::AppHandle,
    net: State<'_, NetManagerState>,
    prepared_id: Uuid,
    peer_id: String,
    peer_name: String,
    selected_file_ids: Vec<u32>,
) -> crate::AppResult<StartSendResult> {
    let transfer = get_transfer(&net).await?;
    transfer.send_offer(&prepared_id, &peer_id, &peer_name, &selected_file_ids, app)
}

/// 确认接收：生成密钥，回复 OfferResult，启动后台拉取
#[tauri::command]
pub async fn accept_receive(
    app: tauri::AppHandle,
    net: State<'_, NetManagerState>,
    session_id: Uuid,
    save_location: entity::SaveLocation,
) -> crate::AppResult<()> {
    let transfer = get_transfer(&net).await?;
    transfer
        .accept_and_start_receive(&session_id, save_location, app)
        .await
}

/// 拒绝接收：回复拒绝的 OfferResult
#[tauri::command]
pub async fn reject_receive(
    net: State<'_, NetManagerState>,
    session_id: Uuid,
) -> crate::AppResult<()> {
    let transfer = get_transfer(&net).await?;
    transfer.reject_and_respond(&session_id).await
}

/// 取消发送
#[tauri::command]
pub async fn cancel_send(
    net: State<'_, NetManagerState>,
    session_id: Uuid,
) -> crate::AppResult<()> {
    let transfer = get_transfer(&net).await?;
    transfer.cancel_send(&session_id).await
}

/// 取消接收
#[tauri::command]
pub async fn cancel_receive(
    db: State<'_, sea_orm::DatabaseConnection>,
    net: State<'_, NetManagerState>,
    session_id: Uuid,
) -> crate::AppResult<()> {
    // 先取消运行时会话（确保 bitmap 已刷写），再标记 DB 状态
    let transfer = get_transfer(&net).await?;
    transfer.cancel_receive(&session_id).await?;

    crate::database::ops::mark_session_cancelled(&db, session_id).await?;
    Ok(())
}

// ============ 传输历史 API ============

/// 查询传输历史列表（可选按状态过滤）
#[tauri::command]
pub async fn get_transfer_history(
    db: State<'_, sea_orm::DatabaseConnection>,
    status: Option<entity::SessionStatus>,
) -> crate::AppResult<Vec<crate::database::ops::TransferHistoryItem>> {
    Ok(crate::database::ops::get_transfer_history(&db, status).await?)
}

/// 查询单个传输会话详情
#[tauri::command]
pub async fn get_transfer_session(
    db: State<'_, sea_orm::DatabaseConnection>,
    session_id: Uuid,
) -> crate::AppResult<crate::database::ops::TransferHistoryItem> {
    Ok(crate::database::ops::get_session_detail(&db, session_id).await?)
}

/// 删除单个传输会话
#[tauri::command]
pub async fn delete_transfer_session(
    db: State<'_, sea_orm::DatabaseConnection>,
    session_id: Uuid,
) -> crate::AppResult<()> {
    Ok(crate::database::ops::delete_session(&db, session_id).await?)
}

/// 清空所有传输历史
#[tauri::command]
pub async fn clear_transfer_history(
    db: State<'_, sea_orm::DatabaseConnection>,
) -> crate::AppResult<()> {
    Ok(crate::database::ops::clear_all_history(&db).await?)
}

/// 暂停传输（自动检测发送/接收方向，通知对端）
#[tauri::command]
pub async fn pause_transfer(
    app: tauri::AppHandle,
    db: State<'_, sea_orm::DatabaseConnection>,
    net: State<'_, NetManagerState>,
    session_id: Uuid,
) -> crate::AppResult<()> {
    let transfer = get_transfer(&net).await?;

    // 尝试暂停发送会话，如果不存在则尝试暂停接收会话
    if transfer.pause_send(&session_id, &app).await.is_err() {
        // 发送会话不存在，尝试接收会话（忽略不存在的错误——可能已被对端取消）
        let _ = transfer.pause_receive(&session_id).await;
    }

    crate::database::ops::mark_session_paused(&db, session_id).await?;

    // 同步 session 级别的 transferred_bytes（从文件记录汇总）
    if let Err(e) = crate::database::ops::sync_session_transferred_bytes(&db, session_id).await {
        tracing::warn!("同步 session 字节数失败: {}", e);
    }

    Ok(())
}

/// 恢复传输结果（返回给前端以创建运行时 session）
#[derive(Debug, Clone, Serialize)]
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

/// 恢复传输（根据方向自动选择接收方或发送方恢复流程）
#[tauri::command]
pub async fn resume_transfer(
    app: tauri::AppHandle,
    db: State<'_, sea_orm::DatabaseConnection>,
    net: State<'_, NetManagerState>,
    session_id: Uuid,
) -> crate::AppResult<ResumeTransferResult> {
    let transfer = get_transfer(&net).await?;

    // 从 DB 读取会话方向
    let session = entity::TransferSession::find_by_id(session_id)
        .one(db.inner())
        .await?
        .ok_or_else(|| crate::AppError::Transfer("会话不存在".into()))?;

    let (resume_info, direction_str) = match session.direction {
        entity::TransferDirection::Receive => {
            let info = transfer.initiate_resume(&db, session_id, app).await?;
            (info, "receive")
        }
        entity::TransferDirection::Send => {
            let info = transfer
                .initiate_resume_as_sender(&db, session_id, app)
                .await?;
            (info, "send")
        }
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

/// 从 Tauri State 中获取 TransferManager（短暂持锁后立即释放）
async fn get_transfer(net: &NetManagerState) -> crate::AppResult<Arc<TransferManager>> {
    let guard = net.lock().await;
    let manager = guard.as_ref().ok_or(crate::AppError::NodeNotStarted)?;
    Ok(manager.transfer_arc())
}
