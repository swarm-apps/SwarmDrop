//! Drop Inbox Tauri 命令薄壳
//!
//! core 负责收件箱数据模型和记录语义；这里仅桥接桌面平台的打开、显示、
//! 导出和删除本地文件操作。

use std::path::{Path, PathBuf};

use sea_orm::DatabaseConnection;
use tauri::State;
use uuid::Uuid;

use crate::database::inbox::{InboxItemDetail, InboxItemFileEntry, InboxItemSummary};

#[tauri::command]
#[specta::specta]
pub async fn list_inbox_items(
    db: State<'_, DatabaseConnection>,
    include_archived: bool,
) -> crate::AppResult<Vec<InboxItemSummary>> {
    Ok(crate::database::inbox::list_inbox_items(&db, include_archived).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn get_inbox_item_detail(
    db: State<'_, DatabaseConnection>,
    item_id: Uuid,
) -> crate::AppResult<Option<InboxItemDetail>> {
    Ok(crate::database::inbox::get_inbox_item_detail(&db, item_id).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn repair_missing_inbox_items(
    db: State<'_, DatabaseConnection>,
) -> crate::AppResult<Vec<InboxItemDetail>> {
    Ok(crate::database::inbox::repair_missing_inbox_items_for_completed_receives(&db).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn open_inbox_item(
    db: State<'_, DatabaseConnection>,
    item_id: Uuid,
    file_id: Option<i32>,
) -> crate::AppResult<()> {
    let detail = load_inbox_detail(&db, item_id).await?;
    let path = item_target_path(&detail, file_id)?;
    ensure_path_exists(&db, &detail, file_id, &path).await?;
    tauri_plugin_opener::open_path(&path, None::<&str>)
        .map_err(|e| crate::AppError::transfer(format!("打开路径失败: {e}")))?;
    crate::database::inbox::mark_inbox_item_opened(&db, item_id).await?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn show_inbox_item_in_folder(
    db: State<'_, DatabaseConnection>,
    item_id: Uuid,
    file_id: Option<i32>,
) -> crate::AppResult<()> {
    let detail = load_inbox_detail(&db, item_id).await?;
    let path = item_target_path(&detail, file_id)?;
    ensure_path_exists(&db, &detail, file_id, &path).await?;
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
    db: State<'_, DatabaseConnection>,
    item_id: Uuid,
    destination_dir: String,
) -> crate::AppResult<()> {
    let detail = load_inbox_detail(&db, item_id).await?;
    let destination_dir = PathBuf::from(destination_dir);
    tokio::fs::create_dir_all(&destination_dir).await?;

    for file in &detail.files {
        let source = PathBuf::from(&file.local_path);
        ensure_file_exists(&db, file, &source).await?;
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
    db: State<'_, DatabaseConnection>,
    item_id: Uuid,
    archived: bool,
) -> crate::AppResult<()> {
    Ok(crate::database::inbox::archive_inbox_item(&db, item_id, archived).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn delete_inbox_item(
    db: State<'_, DatabaseConnection>,
    item_id: Uuid,
    delete_local_files: bool,
) -> crate::AppResult<()> {
    let detail = load_inbox_detail(&db, item_id).await?;
    if delete_local_files {
        for file in &detail.files {
            let path = PathBuf::from(&file.local_path);
            match tokio::fs::remove_file(&path).await {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    crate::database::inbox::mark_inbox_item_file_missing(&db, file.id, true)
                        .await?;
                }
                Err(e) => return Err(e.into()),
            }
        }
    }
    Ok(crate::database::inbox::delete_inbox_item_record(&db, item_id).await?)
}

async fn load_inbox_detail(
    db: &DatabaseConnection,
    item_id: Uuid,
) -> crate::AppResult<InboxItemDetail> {
    crate::database::inbox::get_inbox_item_detail(db, item_id)
        .await?
        .ok_or_else(|| crate::AppError::transfer("收件箱记录不存在"))
}

fn item_target_path(detail: &InboxItemDetail, file_id: Option<i32>) -> crate::AppResult<PathBuf> {
    if let Some(file_id) = file_id {
        let file = detail
            .files
            .iter()
            .find(|file| file.id == file_id)
            .ok_or_else(|| crate::AppError::transfer("收件箱文件不存在"))?;
        return Ok(PathBuf::from(&file.local_path));
    }

    if detail.files.len() == 1 {
        return Ok(PathBuf::from(&detail.files[0].local_path));
    }

    detail
        .item
        .root_path
        .as_ref()
        .map(PathBuf::from)
        .ok_or_else(|| crate::AppError::transfer("收件箱记录缺少本地位置"))
}

async fn ensure_path_exists(
    db: &DatabaseConnection,
    detail: &InboxItemDetail,
    file_id: Option<i32>,
    path: &Path,
) -> crate::AppResult<()> {
    if path.exists() {
        return Ok(());
    }
    if let Some(file_id) = file_id {
        crate::database::inbox::mark_inbox_item_file_missing(db, file_id, true).await?;
    } else if detail.files.len() == 1 {
        crate::database::inbox::mark_inbox_item_file_missing(db, detail.files[0].id, true).await?;
    }
    Err(crate::AppError::transfer(format!(
        "本地文件不存在: {}",
        path.display()
    )))
}

async fn ensure_file_exists(
    db: &DatabaseConnection,
    file: &InboxItemFileEntry,
    path: &Path,
) -> crate::AppResult<()> {
    if path.is_file() {
        return Ok(());
    }
    crate::database::inbox::mark_inbox_item_file_missing(db, file.id, true).await?;
    Err(crate::AppError::transfer(format!(
        "本地文件不存在: {}",
        path.display()
    )))
}
