//! 文件传输 Tauri 命令薄壳
//!
//! 业务逻辑全部在 [`swarmdrop_core::transfer::manager::TransferManager`]，
//! 这里只做：参数转换 + Tauri State / Channel 注册 + 调用 core。

use std::sync::Arc;

use serde::Serialize;
use swarmdrop_core::transfer::HostEnumeratedFile;
use swarmdrop_core::transfer::manager::{StartSendResult, TransferManager};
use swarmdrop_core::transfer::store::TransferProjection;
use tauri::{AppHandle, State};
use uuid::Uuid;

use crate::database::TransferStoreState;
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
pub async fn scan_sources(sources: Vec<FileSource>) -> crate::AppResult<Vec<ScannedSourceResult>> {
    let mut results = Vec::new();
    for source in sources {
        let meta = source.stat().await?;
        if meta.is_dir {
            let entries = source.enumerate(&meta.name).await?;
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
/// 准备发送：一遍流式读产出 checksum + 验签树。
///
/// 进度经 `PrepareProgress` typed event 广播（按返回值里的 `preparedId` 认领），
/// **不再收 `Channel` 入参**——广播才能覆盖 MCP 这类没有 invoke 生命周期的发起方，
/// 也才能让进度在前端离开发送页后继续可读。理由见 `crate::events::PrepareProgress`。
///
/// 注意事件先于本命令的返回值到达前端，所以 `preparedId` 拿不到「提前」：前端只能由
/// 首条事件自我认领。
pub async fn prepare_send(
    net: State<'_, NetManagerState>,
    files: Vec<EnumeratedFile>,
) -> crate::AppResult<PreparedTransferResult> {
    let transfer = get_transfer(&net).await?;
    let prepared_id = Uuid::new_v4();

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

// 暂停与取消一样按方向分两条：方向是投影里现成的事实，前端直接分派，
// 后端不做「先试发送失败再试接收」的试错（那会把一条真实错误藏进两串拼接文案里）。

#[tauri::command]
#[specta::specta]
pub async fn pause_send(net: State<'_, NetManagerState>, session_id: Uuid) -> crate::AppResult<()> {
    let transfer = get_transfer(&net).await?;
    Ok(transfer.pause_send(&session_id).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn pause_receive(
    net: State<'_, NetManagerState>,
    session_id: Uuid,
) -> crate::AppResult<()> {
    let transfer = get_transfer(&net).await?;
    Ok(transfer.pause_receive(&session_id).await?)
}

// ============ 传输投影 API ============
//
// 查询类命令吃的是**账本端口**（`TransferStoreState`，组装点在 `setup.rs` 就 manage 好了），
// 不经 `TransferManager`。传输历史与节点在不在跑无关：走网络层意味着 start 失败或还没
// start 时，前端一挂载就整页 `node_not_started`——而它要的只是读一张表。

#[tauri::command]
#[specta::specta]
pub async fn get_transfer_projections(
    store: State<'_, TransferStoreState>,
) -> crate::AppResult<Vec<TransferProjection>> {
    Ok(store.list_transfer_projections().await?)
}

/// 删除单条传输记录。走域方法而非 `store().delete_session()`——「进行中不可删」的守卫在那里。
#[tauri::command]
#[specta::specta]
pub async fn delete_transfer_session(
    net: State<'_, NetManagerState>,
    session_id: Uuid,
) -> crate::AppResult<()> {
    let transfer = get_transfer(&net).await?;
    Ok(transfer.delete_session(session_id).await?)
}

/// 清空传输历史：只删已终态的记录，进行中与可续传的会话保留（端口契约）。
#[tauri::command]
#[specta::specta]
pub async fn clear_transfer_history(store: State<'_, TransferStoreState>) -> crate::AppResult<()> {
    Ok(store.clear_all_history().await?)
}

/// 发送方向会话的源文件绝对路径（「重新发送」重建载荷用；接收方向返回空列表）。
#[tauri::command]
#[specta::specta]
pub async fn get_transfer_source_paths(
    store: State<'_, TransferStoreState>,
    session_id: Uuid,
) -> crate::AppResult<Vec<String>> {
    Ok(store.get_session_source_paths(session_id).await?)
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
    net: State<'_, NetManagerState>,
    session_id: Uuid,
) -> crate::AppResult<ResumeTransferResult> {
    let transfer = get_transfer(&net).await?;

    let session = transfer
        .store()
        .find_session(session_id)
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

pub(super) async fn get_transfer(net: &NetManagerState) -> crate::AppResult<Arc<TransferManager>> {
    let guard = net.lock().await;
    let manager = guard.as_ref().ok_or(crate::AppError::node_not_started())?;
    Ok(manager.transfer_arc())
}
