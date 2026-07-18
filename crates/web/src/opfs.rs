//! OPFS 基础原语（与 [`FileAccess`](swarmdrop_host::FileAccess) 端口无关的通用层）。
//!
//! 层次：`env`（环境探测）→ 本模块（OPFS 读写原语）→ `file_access` / `identity`（各自的
//! 端口/持久化实现）。所有入口都过 [`ensure_secure_context`] 预检 + 5s 超时兜底
//! （非 secure 源下 `navigator.storage` 是 undefined，JsFuture 会**永久 pending**）。
//!
//! JsValue `!Send` 的 Send 兜底纪律见 `file_access` 模块 doc（本模块的 async fn 内部
//! 直接跨 await 持 !Send 句柄，由调用方整段裹 `SendWrapper`）。

use std::time::Duration;

use send_wrapper::SendWrapper;
use swarmdrop_host::{AppError, AppResult};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    File, FileSystemCreateWritableOptions, FileSystemDirectoryHandle, FileSystemFileHandle,
    FileSystemGetDirectoryOptions, FileSystemGetFileOptions, FileSystemWritableFileStream,
};

/// 预检 secure context。
///
/// **非 secure 源（http 私网 IP 等）下 `navigator.storage` 是 undefined，其 `getDirectory()`
/// 的 JsFuture 永久 pending（不 resolve 不 reject）→ finalize 静默挂死**（最坏失败模式：无
/// 错误、无超时；我们实测时花了两轮才定位）。提前明确报错。secure context 仅含 https /
/// localhost / 127.0.0.1（见 libp2p-wasm.md：非 secure 源同时丢 crypto.subtle 与 OPFS）。
fn ensure_secure_context() -> AppResult<()> {
    if !crate::env::is_secure_context() {
        return Err(AppError::Transfer(
            "OPFS 不可用：当前页面非 secure context，navigator.storage 缺失。请用 https 或 \
             localhost / 127.0.0.1 访问（而非 http 私网 IP）。"
                .into(),
        ));
    }
    Ok(())
}

/// OPFS 根目录（`navigator.storage.getDirectory()`，Window / Worker 通吃）。
async fn opfs_root() -> AppResult<FileSystemDirectoryHandle> {
    ensure_secure_context()?;
    let storage = crate::env::storage_manager()
        .ok_or_else(|| AppError::Transfer("navigator.storage 不可达（未知全局环境）".into()))?;
    // secure context 下仍加超时兜底：任何底层不 resolve 都在 5s 内明确失败，绝不永久挂起。
    let root = match n0_future::time::timeout(
        Duration::from_secs(5),
        SendWrapper::new(JsFuture::from(storage.get_directory())),
    )
    .await
    {
        Ok(r) => r.map_err(js_to_err)?,
        Err(_) => {
            return Err(AppError::Transfer(
                "OPFS getDirectory 5s 超时——navigator.storage 未响应（非 secure context？）".into(),
            ));
        }
    };
    root.dyn_into::<FileSystemDirectoryHandle>()
        .map_err(|_| AppError::Transfer("getDirectory 返回非目录句柄".into()))
}

/// 沿 `relative_path` 逐段建目录，返回末段文件句柄（`create:true`）。
pub(crate) async fn opfs_file_handle(
    relative_path: &str,
    create: bool,
) -> AppResult<FileSystemFileHandle> {
    let mut dir = opfs_root().await?;
    let parts: Vec<&str> = relative_path.split('/').filter(|s| !s.is_empty()).collect();
    let (file_name, dirs) = parts
        .split_last()
        .ok_or_else(|| AppError::Transfer("空 relative_path".into()))?;
    for seg in dirs {
        let opts = FileSystemGetDirectoryOptions::new();
        opts.set_create(create);
        let handle = SendWrapper::new(JsFuture::from(
            dir.get_directory_handle_with_options(seg, &opts),
        ))
        .await
        .map_err(js_to_err)?;
        dir = handle
            .dyn_into::<FileSystemDirectoryHandle>()
            .map_err(|_| AppError::Transfer("子目录句柄类型错误".into()))?;
    }
    let opts = FileSystemGetFileOptions::new();
    opts.set_create(create);
    let handle = SendWrapper::new(JsFuture::from(
        dir.get_file_handle_with_options(file_name, &opts),
    ))
    .await
    .map_err(js_to_err)?;
    handle
        .dyn_into::<FileSystemFileHandle>()
        .map_err(|_| AppError::Transfer("文件句柄类型错误".into()))
}

/// 打开 `relative_path` 的 OPFS 流式写句柄。`keep_existing_data=true` 保留已有内容（续传用），
/// false 打开即截断。句柄由调用方持有，逐块 positioned write、最后 `close` 提交。
pub(crate) async fn open_writable(
    relative_path: &str,
    keep_existing_data: bool,
) -> AppResult<FileSystemWritableFileStream> {
    let handle = opfs_file_handle(relative_path, true).await?;
    // !Send 的 opts 只在 block 内构造并取到 Promise 后即丢，只让 SendWrapper<JsFuture> 跨 await。
    let create_promise = {
        let opts = FileSystemCreateWritableOptions::new();
        opts.set_keep_existing_data(keep_existing_data);
        handle.create_writable_with_options(&opts)
    };
    SendWrapper::new(JsFuture::from(create_promise))
        .await
        .map_err(js_to_err)?
        .dyn_into::<FileSystemWritableFileStream>()
        .map_err(|_| AppError::Transfer("createWritable 返回类型错误".into()))
}

/// 读回 OPFS 文件建 blob URL（供 JS `<a download>` 下载）。demo 用。
///
/// **快速失败**：文件不存在（会话未完成/不存在）→ `get_file_handle(create:false)` 立即 reject
/// → 返回错误。外加超时兜底——保证**永不永久挂起**（team-lead 实测到 1800s+ 挂死，用超时封顶：
/// 无论底层 OPFS 因何不响应，都在 5s 内明确失败，而非 await 永不解决）。
pub async fn export_blob_url(relative_path: &str) -> AppResult<String> {
    match n0_future::time::timeout(Duration::from_secs(5), export_blob_url_inner(relative_path))
        .await
    {
        Ok(result) => result,
        Err(_) => Err(AppError::Transfer(
            "下载失败：OPFS 文件未就绪（会话未完成？）——5s 超时".into(),
        )),
    }
}

async fn export_blob_url_inner(relative_path: &str) -> AppResult<String> {
    let handle = opfs_file_handle(relative_path, false).await?;
    let file = SendWrapper::new(JsFuture::from(handle.get_file()))
        .await
        .map_err(js_to_err)?
        .dyn_into::<File>()
        .map_err(|_| AppError::Transfer("getFile 返回类型错误".into()))?;
    web_sys::Url::create_object_url_with_blob(&file).map_err(js_to_err)
}

/// JS 错误 → [`AppError`]（OPFS/JS 语境的通用收敛）。
pub(crate) fn js_to_err(v: JsValue) -> AppError {
    AppError::Transfer(format!("OPFS/JS 错误: {v:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

    /// Bug 2 回归：对未就绪（会话未完成 / 文件不存在）的路径，`export_blob_url` 必须**返回**
    /// 一个 `Err`，绝不永久挂起（team-lead 实测到 1800s+ 挂死）。
    ///
    /// 本测试自身能跑完即证明「不永久挂起」：export_blob_url 内 5s 超时封顶，无论 OPFS 因何
    /// 不响应（含 harness 里 OPFS 不可用时 opfs_root 直接报错）都会在有限时间内落到 `Err`。
    /// 跑法：`wasm-pack test --headless --chrome -p swarmdrop-web`。
    #[wasm_bindgen_test]
    async fn export_blob_url_missing_file_fails_fast() {
        let result = export_blob_url("does-not-exist/never-written.bin").await;
        assert!(
            result.is_err(),
            "未就绪路径应快速失败返回 Err，实际: {result:?}"
        );
    }
}
