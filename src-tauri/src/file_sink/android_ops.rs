//! Android 端文件写入操作
//!
//! 通过 `tauri-plugin-android-fs` 的 PublicStorage API 将接收的文件保存到公共 Download 目录。
//! 利用 pending 机制（Android 10+）：文件在写入期间对其他应用不可见，校验通过后才公开。
//!
//! 分块写入已由 `PartFile::write_chunk()` 统一处理（跨平台 pwrite），
//! 此模块仅负责创建文件（含缓存句柄）、校验最终化和清理。

use std::path::PathBuf;

use tauri_plugin_android_fs::{AndroidFsExt, FileAccessMode, FileUri, PublicGeneralPurposeDir};
use tracing::warn;

use crate::file_sink::PartFile;
use crate::{AppError, AppResult};

/// 请求写入权限（Android 9 及以下需要）
///
/// Android 11+ 自动授予，此方法直接返回 Ok。
/// Android 9-10 弹出系统权限对话框。
pub async fn ensure_permission(app: &tauri::AppHandle) -> AppResult<()> {
    let granted = app
        .android_fs_async()
        .public_storage()
        .request_permission()
        .await
        .map_err(|e| AppError::Transfer(format!("请求存储权限失败: {e}")))?;

    if !granted {
        return Err(AppError::Transfer("用户拒绝了存储权限".into()));
    }
    Ok(())
}

/// 创建文件（pending 状态）并返回带缓存句柄的 PartFile
///
/// 使用 `create_new_file_with_pending` 在 Download/{subdir}/ 下创建文件，
/// 文件在 pending 状态下对其他应用不可见。
/// 打开文件句柄并缓存，后续 `PartFile::write_chunk()` 直接使用 pwrite 写入。
pub async fn create_part_file(
    subdir: &str,
    relative_path: &str,
    file_size: u64,
    app: &tauri::AppHandle,
) -> AppResult<PartFile> {
    let full_relative = format!("{subdir}/{relative_path}");

    let file_uri = app
        .android_fs_async()
        .public_storage()
        .create_new_file_with_pending(
            None, // 使用主存储卷
            PublicGeneralPurposeDir::Download,
            &full_relative,
            None, // 从扩展名推断 MIME 类型
        )
        .await
        .map_err(|e| AppError::Transfer(format!("Android 创建文件失败: {relative_path}, {e}")))?;

    // 打开文件并缓存句柄（用于后续 pwrite 写入分块）
    let file = app
        .android_fs_async()
        .open_file(&file_uri, FileAccessMode::ReadWrite)
        .await
        .map_err(|e| AppError::Transfer(format!("Android 打开文件失败: {relative_path}, {e}")))?;

    // 预分配文件大小：提前检查磁盘空间，避免传输到一半才失败
    if file_size > 0 {
        let f = file.try_clone().map_err(|e| {
            AppError::Transfer(format!("Android clone 文件句柄失败: {relative_path}, {e}"))
        })?;
        tokio::task::spawn_blocking(move || f.set_len(file_size))
            .await?
            .map_err(|e: std::io::Error| {
                AppError::Transfer(format!("Android 预分配文件大小失败: {relative_path}, {e}"))
            })?;
    }

    Ok(PartFile::new_android(
        PathBuf::from(relative_path),
        file_size,
        file_uri,
        file,
    ))
}

/// 校验 BLAKE3 并最终化文件
///
/// 1. 以只读模式打开文件，流式计算 BLAKE3 hash
/// 2. 校验通过：`set_pending(false)` 使文件可见 + `scan()` 刷新 MediaStore
/// 3. 校验失败：`remove_file()` 删除文件
///
/// 调用前需确保写入句柄已关闭（`PartFile::close_write_handle()`）。
pub async fn verify_and_finalize(
    part_file: &PartFile,
    expected_checksum: &str,
    app: &tauri::AppHandle,
) -> AppResult<PathBuf> {
    let file_uri = part_file
        .file_uri
        .as_ref()
        .ok_or_else(|| AppError::Transfer("PartFile 缺少 file_uri（Android）".into()))?;

    // 计算 BLAKE3 hash
    let mut file = app
        .android_fs_async()
        .open_file_readable(file_uri)
        .await
        .map_err(|e| AppError::Transfer(format!("Android 打开文件失败（校验）: {e}")))?;

    let expected = expected_checksum.to_owned();
    let checksum_ok = tokio::task::spawn_blocking(move || {
        let mut hasher = blake3::Hasher::new();
        hasher
            .update_reader(&mut file)
            .map_err(|e| AppError::Transfer(format!("Android 校验读取失败: {e}")))?;
        let actual_hex = hasher.finalize().to_hex().to_string();
        Ok::<bool, AppError>(actual_hex == expected)
    })
    .await??;

    if !checksum_ok {
        // 校验失败，删除文件
        let _ = app.android_fs_async().remove_file(file_uri).await;
        return Err(AppError::Transfer(format!(
            "文件校验失败: {}",
            part_file.final_path.display()
        )));
    }

    // 校验通过：取消 pending 状态，使文件对其他应用可见
    app.android_fs_async()
        .public_storage()
        .set_pending(file_uri, false)
        .await
        .map_err(|e| AppError::Transfer(format!("Android set_pending(false) 失败: {e}")))?;

    // 刷新 MediaStore 索引
    app.android_fs_async()
        .public_storage()
        .scan(file_uri)
        .await
        .map_err(|e| AppError::Transfer(format!("Android MediaStore scan 失败: {e}")))?;

    Ok(part_file.final_path.clone())
}

/// 清理文件（静默忽略错误）
///
/// 删除 pending 状态的文件。如果文件已被最终化或不存在，忽略错误。
pub async fn cleanup_part_file(part_file: &PartFile, app: &tauri::AppHandle) {
    if let Some(file_uri) = &part_file.file_uri {
        if let Err(e) = app.android_fs_async().remove_file(file_uri).await {
            warn!("Android 清理文件失败（已忽略）: {e}");
        }
    }
}

/// 获取保存目录的 FileUri
///
/// 通过 `resolve_initial_location` 获取 `Download/{subdir}` 目录的标准 content URI。
/// 前端可直接用于 `showViewDirDialog`。
pub async fn resolve_save_dir_uri(subdir: &str, app: &tauri::AppHandle) -> Option<FileUri> {
    app.android_fs_async()
        .public_storage()
        .resolve_initial_location(
            None, // 主存储卷
            PublicGeneralPurposeDir::Download,
            subdir,
            false, // 目录已由文件写入时自动创建
        )
        .await
        .ok()
}
