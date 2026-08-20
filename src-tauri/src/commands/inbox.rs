//! Drop Inbox Tauri 命令薄壳
//!
//! 收件箱的数据模型与记录语义在 `swarmdrop-transfer` 的 `InboxStore` 端口上；
//! 这里只做两件事：把查询与标记转发给端口，以及桥接桌面平台特有的动作
//! （`tauri_plugin_opener` 打开 / 在文件夹中显示、`tokio::fs` 导出与删除本地文件）。
//! 平台动作**留在命令里**，端口只认「查询 + 标记」。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use swarmdrop_core::host::FileAccess;
use swarmdrop_core::transfer::inbox::{
    InboxItemContent, InboxItemDetail, InboxItemSummary, InboxSearchHit,
};
use tauri::State;
use uuid::Uuid;

use crate::database::TransferStoreState;

#[tauri::command]
#[specta::specta]
pub async fn list_inbox_items(
    store: State<'_, TransferStoreState>,
    include_archived: bool,
) -> crate::AppResult<Vec<InboxItemSummary>> {
    Ok(store.list_inbox_items(include_archived).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn get_inbox_item_detail(
    store: State<'_, TransferStoreState>,
    item_id: Uuid,
) -> crate::AppResult<Option<InboxItemDetail>> {
    Ok(store.get_inbox_item_detail(item_id).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn get_inbox_item_by_transfer_session_id(
    store: State<'_, TransferStoreState>,
    session_id: Uuid,
) -> crate::AppResult<Option<InboxItemDetail>> {
    Ok(store
        .get_inbox_item_by_transfer_session_id(session_id)
        .await?)
}

/// 检索收件箱。`limit` 缺省取三端共享的 [`INBOX_SEARCH_LIMIT`]，`include_archived` 默认 false。
///
/// 存储端口未注入时由 Tauri State 注入机制直接返回错误（不会 panic）。
#[tauri::command]
#[specta::specta]
pub async fn search_inbox(
    store: State<'_, TransferStoreState>,
    query: String,
    limit: Option<u32>,
    include_archived: Option<bool>,
) -> crate::AppResult<Vec<InboxSearchHit>> {
    Ok(store
        .search_inbox_capped(&query, limit, include_archived.unwrap_or(false))
        .await?)
}

#[tauri::command]
#[specta::specta]
pub async fn repair_missing_inbox_items(
    store: State<'_, TransferStoreState>,
) -> crate::AppResult<Vec<InboxItemDetail>> {
    Ok(store
        .repair_missing_inbox_items_for_completed_receives()
        .await?)
}

#[tauri::command]
#[specta::specta]
pub async fn open_inbox_item(
    store: State<'_, TransferStoreState>,
    item_id: Uuid,
    file_id: Option<i32>,
) -> crate::AppResult<()> {
    let detail = load_inbox_detail(&store, item_id).await?;
    let path = item_target_path(&detail, file_id)?;
    ensure_local_target_exists(&store, item_id, missing_file_id(&detail, file_id), &path).await?;
    tauri_plugin_opener::open_path(&path, None::<&str>)
        .map_err(|e| crate::AppError::transfer(format!("打开路径失败: {e}")))?;
    store.mark_inbox_item_opened(item_id).await?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn show_inbox_item_in_folder(
    store: State<'_, TransferStoreState>,
    item_id: Uuid,
    file_id: Option<i32>,
) -> crate::AppResult<()> {
    let detail = load_inbox_detail(&store, item_id).await?;
    let path = item_target_path(&detail, file_id)?;
    ensure_local_target_exists(&store, item_id, missing_file_id(&detail, file_id), &path).await?;
    if path.is_file() {
        tauri_plugin_opener::reveal_item_in_dir(&path)
            .map_err(|e| crate::AppError::transfer(format!("显示文件位置失败: {e}")))?;
    } else {
        tauri_plugin_opener::open_path(&path, None::<&str>)
            .map_err(|e| crate::AppError::transfer(format!("打开文件夹失败: {e}")))?;
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn export_inbox_item(
    store: State<'_, TransferStoreState>,
    item_id: Uuid,
    destination_dir: String,
) -> crate::AppResult<()> {
    let detail = load_inbox_detail(&store, item_id).await?;
    let destination_dir = PathBuf::from(destination_dir);
    tokio::fs::create_dir_all(&destination_dir).await?;

    let InboxItemContent::Files { entries, .. } = &detail.content else {
        return Err(crate::AppError::transfer("文本收件箱不能导出为文件"));
    };
    for file in entries {
        let source = PathBuf::from(&file.local_path);
        ensure_local_target_exists(&store, item_id, Some(file.id), &source).await?;
        let destination = destination_dir.join(&file.relative_path);
        if let Some(parent) = destination.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::copy(&source, &destination).await?;
    }

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn archive_inbox_item(
    store: State<'_, TransferStoreState>,
    item_id: Uuid,
    archived: bool,
) -> crate::AppResult<()> {
    Ok(store.archive_inbox_item(item_id, archived).await?)
}

/// 删除收件箱条目；`delete_local_files` 为真时连已落盘的文件一起删。
///
/// 编排（先文件后记录、删文件失败不阻断、条目不存在报错）是**三端共用的领域规则**，
/// 住在 [`swarmdrop_transfer::inbox::delete_inbox_item`]。此前这里裸写
/// `tokio::fs::remove_file` —— 那是绕过 `FileAccess` 端口的第三份删除实现，
/// 现已收编到 `LocalFileAccess::delete_finalized_file`。
///
/// `FileAccess` 从 state 取而不是现建一个：与 `start()` 注入给 `TransferManager` 的
/// **是同一个 `Arc`**（组装点在 `setup.rs`）。收件箱命令刻意不经 `TransferManager`
/// ——它是与网络无关的内容账本，绑到节点生命周期上会变成「没联网就翻不了已收到的东西」。
#[tauri::command]
#[specta::specta]
pub async fn delete_inbox_item(
    store: State<'_, TransferStoreState>,
    file_access: State<'_, Arc<dyn FileAccess>>,
    item_id: Uuid,
    delete_local_files: bool,
) -> crate::AppResult<()> {
    Ok(swarmdrop_core::transfer::inbox::delete_inbox_item(
        store.inner().as_ref(),
        file_access.inner().as_ref(),
        item_id,
        delete_local_files,
    )
    .await?)
}

async fn load_inbox_detail(
    store: &TransferStoreState,
    item_id: Uuid,
) -> crate::AppResult<InboxItemDetail> {
    store
        .get_inbox_item_detail(item_id)
        .await?
        .ok_or_else(|| crate::AppError::transfer("收件箱记录不存在"))
}

fn item_target_path(detail: &InboxItemDetail, file_id: Option<i32>) -> crate::AppResult<PathBuf> {
    let InboxItemContent::Files { entries, .. } = &detail.content else {
        return Err(crate::AppError::transfer("文本收件箱没有本地文件位置"));
    };
    if let Some(file_id) = file_id {
        let file = entries
            .iter()
            .find(|file| file.id == file_id)
            .ok_or_else(|| crate::AppError::transfer("收件箱文件不存在"))?;
        return Ok(PathBuf::from(&file.local_path));
    }

    if entries.len() == 1 {
        return Ok(PathBuf::from(&entries[0].local_path));
    }

    detail
        .item
        .root_path
        .as_ref()
        .map(PathBuf::from)
        .ok_or_else(|| crate::AppError::transfer("收件箱记录缺少本地位置"))
}

/// 账本说有、磁盘上没有：回写 missing 标志再报错，让列表与磁盘保持一致。
///
/// `missing_file_id` 是「这次缺失该记在哪个文件行上」，由调用方决定：导出逐文件遍历时
/// 就是那个文件；打开 / 定位则见 [`missing_file_id`]。整条条目的根目录缺失（多文件条目
/// 未指定 file_id）时为 `None` —— 没有单个文件可归咎，只报错不改标志。
///
/// 判据统一用 `exists()`（原先导出侧另用 `is_file()`）：这个检查回答的是「账本指的东西
/// 还在不在」，路径存在但不是普通文件属于账本损坏，不该套用「本地文件不存在」这句话。
async fn ensure_local_target_exists(
    store: &TransferStoreState,
    item_id: Uuid,
    missing_file_id: Option<i32>,
    path: &Path,
) -> crate::AppResult<()> {
    if path.exists() {
        return Ok(());
    }
    if let Some(file_id) = missing_file_id {
        store
            .mark_inbox_item_file_missing(item_id, file_id, true)
            .await?;
    }
    Err(crate::AppError::transfer(format!(
        "本地文件不存在: {}",
        path.display()
    )))
}

/// 未指定 file_id 时，单文件条目的缺失就记在那唯一的文件上。
///
/// **这是桌面独有的兜底**，移动与 Web 都没有——它其实是一条领域规则（「单文件条目的
/// 条目级缺失 == 该文件缺失」），本该上提到 `swarmdrop_transfer::inbox` 供三端共用，
/// 而不是住在命令壳里。留在这里是因为本轮不动 `crates/`。
fn missing_file_id(detail: &InboxItemDetail, file_id: Option<i32>) -> Option<i32> {
    let InboxItemContent::Files { entries, .. } = &detail.content else {
        return None;
    };
    file_id.or_else(|| (entries.len() == 1).then(|| entries[0].id))
}
