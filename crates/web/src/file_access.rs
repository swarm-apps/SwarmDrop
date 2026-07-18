//! 文件访问端口的 Web 实现（[`FileAccess`]）。
//!
//! - **发送侧源**：用户经 `<input type=file>` 选的 [`web_sys::File`] 存 `FileSourceId → File`
//!   映射；`read_source_chunk` 走 `File.slice(offset,end).arrayBuffer()` 读 range。
//! - **接收侧 sink**：**流式落盘**——`create_sink` 时开一个 OPFS `createWritable` 句柄并持有，
//!   每个 chunk 用 `WriteParams { position, data }` **positioned 直写**（单次 Promise 往返），
//!   `finalize_sink` 时 `close`。不再整文件缓冲入内存（大文件不再 OOM）。用主线程 async API
//!   `navigator.storage.getDirectory → createWritable`；
//!   **禁用 SyncAccessHandle**——它 Worker-only，与 webrtc-websys 的主线程约束冲突（知识库有记录）；
//!   Worker 版走 SyncAccessHandle 是另一个 bundle。完成后经 [`export_blob_url`] 读回建 blob URL 供下载。
//!
//! JsValue `!Send`，而 [`FileAccess`] 是 `Send`：用 `send_wrapper::SendWrapper` 兜 Send（单线程
//! wasm 永不触发其跨线程 panic）。映射表（含持有的 writable 句柄）裹 `SendWrapper<RefCell<..>>`
//! 满足 Send+Sync。纪律：**裸 `RefCell` borrow / 裸 !Send 句柄绝不跨 await**；需要跨 await 的
//! !Send 状态一律裹进 SendWrapper——短路径在 scope 内取出 Promise 即丢、只让
//! `SendWrapper<JsFuture>` 跨 await；多步 helper（如 [`open_writable`]）则整段 future 裹 SendWrapper。

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use send_wrapper::SendWrapper;
use swarmdrop_host::{
    AppError, AppResult, FileAccess, FileSinkId, FileSourceId, FinalizedSink, HostFileMetadata,
};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    File, FileSystemCreateWritableOptions, FileSystemDirectoryHandle, FileSystemFileHandle,
    FileSystemGetDirectoryOptions, FileSystemGetFileOptions, FileSystemWritableFileStream,
    WriteCommandType, WriteParams,
};

/// OPFS + File 源的 [`FileAccess`] 实现。
pub struct OpfsFileAccess {
    sources: SendWrapper<RefCell<HashMap<FileSourceId, File>>>,
    /// 接收侧流式写句柄：`create_sink` 时开、每 chunk positioned 直写、`finalize` 时 close。
    /// key（[`FileSinkId`]）就是 relative_path，无需另存。
    sinks: SendWrapper<RefCell<HashMap<FileSinkId, FileSystemWritableFileStream>>>,
}

impl Default for OpfsFileAccess {
    fn default() -> Self {
        Self::new()
    }
}

impl OpfsFileAccess {
    pub fn new() -> Self {
        Self {
            sources: SendWrapper::new(RefCell::new(HashMap::new())),
            sinks: SendWrapper::new(RefCell::new(HashMap::new())),
        }
    }

    /// 登记一个发送源（用户选的 File），返回其 [`FileSourceId`]（用 relative-path 作 id）。
    pub fn register_source(&self, id: FileSourceId, file: File) {
        self.sources.borrow_mut().insert(id, file);
    }
}

#[async_trait]
impl FileAccess for OpfsFileAccess {
    async fn source_metadata(&self, source: &FileSourceId) -> AppResult<HostFileMetadata> {
        let file = self.source(source)?;
        Ok(HostFileMetadata {
            name: file.name(),
            relative_path: source.0.clone(),
            size: file.size() as u64,
            modified_at: Some(file.last_modified() as i64),
            checksum: None,
            save_dir: None,
        })
    }

    async fn read_source_chunk(
        &self,
        source: &FileSourceId,
        offset: u64,
        length: usize,
    ) -> AppResult<Vec<u8>> {
        // 所有 !Send 的 JsValue 都在 await 前拿到 promise 后即丢，只让 SendWrapper<JsFuture>
        // 跨 await——保证方法返回的 future 满足端口的 Send。
        let promise = {
            let file = self.source(source)?;
            let end = offset + length as u64;
            let blob = file
                .slice_with_f64_and_f64(offset as f64, end as f64)
                .map_err(js_to_err)?;
            blob.array_buffer()
        };
        let buf = SendWrapper::new(JsFuture::from(promise))
            .await
            .map_err(js_to_err)?;
        Ok(js_sys::Uint8Array::new(&buf).to_vec())
    }

    async fn create_sink(&self, metadata: HostFileMetadata) -> AppResult<FileSinkId> {
        // 全新文件：keep_existing_data=false，打开即截断任何同名残留。
        self.open_and_store(FileSinkId(metadata.relative_path), false)
            .await
    }

    async fn open_or_create_sink(&self, metadata: HostFileMetadata) -> AppResult<FileSinkId> {
        // 续传：同一会话内已开句柄则复用；否则开 keep_existing_data=true 的句柄保留已落盘部分
        // （positioned write 只覆盖后续 range）。跨页面刷新的续传不在范围内。
        let sink = FileSinkId(metadata.relative_path);
        if self.sinks.borrow().contains_key(&sink) {
            return Ok(sink);
        }
        self.open_and_store(sink, true).await
    }

    async fn write_sink_chunk(
        &self,
        sink: &FileSinkId,
        offset: u64,
        data: Vec<u8>,
    ) -> AppResult<()> {
        // positioned write：WriteParams { type:"write", position, data } 单次调用等价 seek+write，
        // 每 chunk 只走一次 JS Promise 往返。句柄与 params 在 scope 内取到 Promise 即丢。
        let promise = {
            let writable = self.sink(sink)?;
            let params = WriteParams::new(WriteCommandType::Write);
            params.set_position(Some(offset as f64));
            params.set_data(&JsValue::from(js_sys::Uint8Array::from(data.as_slice())));
            writable
                .write_with_write_params(&params)
                .map_err(js_to_err)?
        };
        SendWrapper::new(JsFuture::from(promise))
            .await
            .map_err(js_to_err)?;
        Ok(())
    }

    async fn finalize_sink(&self, sink: &FileSinkId) -> AppResult<FinalizedSink> {
        // 流式直写已把全部 chunk 落盘，finalize 只需 close 句柄提交、再从表中移除。
        let close_promise = self.sink(sink)?.close();
        SendWrapper::new(JsFuture::from(close_promise))
            .await
            .map_err(js_to_err)?;
        self.sinks.borrow_mut().remove(sink);
        let relative_path = &sink.0;
        let dir = relative_path
            .rsplit_once('/')
            .map(|(d, _)| d)
            .unwrap_or_default();
        Ok(FinalizedSink {
            uri: format!("opfs:/{relative_path}"),
            dir: format!("opfs:/{dir}"),
        })
    }

    async fn cleanup_sink(&self, sink: &FileSinkId) -> AppResult<()> {
        // 移除即 drop writable 句柄；未 close 的写入被丢弃——正是取消/失败时该有的行为。
        self.sinks.borrow_mut().remove(sink);
        Ok(())
    }
}

impl OpfsFileAccess {
    fn source(&self, source: &FileSourceId) -> AppResult<File> {
        self.sources
            .borrow()
            .get(source)
            .cloned()
            .ok_or_else(|| AppError::Transfer(format!("文件源不存在: {}", source.0)))
    }

    /// 查表取 sink 的写句柄（clone 只是 wasm-bindgen 堆表引用计数，非数据拷贝）。
    fn sink(&self, sink: &FileSinkId) -> AppResult<FileSystemWritableFileStream> {
        self.sinks
            .borrow()
            .get(sink)
            .cloned()
            .ok_or_else(|| AppError::Transfer(format!("sink 不存在: {}", sink.0)))
    }

    /// 开 `sink.0`（即 relative_path）的流式写句柄并登记。
    /// open_writable 内部有 !Send 句柄跨 await，整体裹 SendWrapper 满足 trait 的 Send 约束。
    async fn open_and_store(
        &self,
        sink: FileSinkId,
        keep_existing_data: bool,
    ) -> AppResult<FileSinkId> {
        let writable = SendWrapper::new(open_writable(&sink.0, keep_existing_data)).await?;
        self.sinks.borrow_mut().insert(sink.clone(), writable);
        Ok(sink)
    }
}

/// 预检 secure context。
///
/// **非 secure 源（http 私网 IP 等）下 `navigator.storage` 是 undefined，其 `getDirectory()`
/// 的 JsFuture 永久 pending（不 resolve 不 reject）→ finalize 静默挂死**（最坏失败模式：无
/// 错误、无超时；我们实测时花了两轮才定位）。提前明确报错。secure context 仅含 https /
/// localhost / 127.0.0.1（见 libp2p-wasm.md：非 secure 源同时丢 crypto.subtle 与 OPFS）。
fn ensure_secure_context() -> AppResult<()> {
    let win = web_sys::window().ok_or_else(|| AppError::Transfer("无 window".into()))?;
    if !win.is_secure_context() {
        return Err(AppError::Transfer(
            "OPFS 不可用：当前页面非 secure context，navigator.storage 缺失。请用 https 或 \
             localhost / 127.0.0.1 访问（而非 http 私网 IP）。"
                .into(),
        ));
    }
    Ok(())
}

/// OPFS 根目录（`navigator.storage.getDirectory()`）。
async fn opfs_root() -> AppResult<FileSystemDirectoryHandle> {
    ensure_secure_context()?;
    let storage = web_sys::window()
        .ok_or_else(|| AppError::Transfer("无 window".into()))?
        .navigator()
        .storage();
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
async fn opfs_file_handle(relative_path: &str, create: bool) -> AppResult<FileSystemFileHandle> {
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
/// false 打开即截断。句柄由调用方持有，逐块 `seek+write`、最后 `close`。
async fn open_writable(
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

fn js_to_err(v: JsValue) -> AppError {
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
