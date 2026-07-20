//! 文件访问端口的 Web 实现（[`FileAccess`]）。
//!
//! - **发送侧源**：用户经 `<input type=file>` 选的 [`web_sys::File`] 存 `FileSourceId → File`
//!   映射；`read_source_chunk` 走 `File.slice(offset,end).arrayBuffer()` 读 range。
//! - **接收侧 sink**：**流式落盘**——`create_sink` 时开一个 OPFS `createWritable` 句柄并持有
//!   （Window / Worker 通吃），每 chunk 用 `WriteParams { position, data }` **positioned 直写**
//!   （单次 Promise 往返），`finalize_sink` 时 `close` 提交。不整文件缓冲入内存（大文件不 OOM）。
//!   曾试过 Worker 侧用 SyncAccessHandle——**实测吞吐无增益**（瓶颈在网络链路），为免双写法
//!   已移除（语义差异与坑见知识库 libp2p-wasm.md）。完成后经 [`crate::opfs::export_blob_url`]
//!   读回建 blob URL 供下载。OPFS 原语（开句柄/建目录链/blob 导出）在 [`crate::opfs`]。
//!
//! JsValue `!Send`，而 [`FileAccess`] 是 `Send`：用 `send_wrapper::SendWrapper` 兜 Send（单线程
//! wasm 永不触发其跨线程 panic）。映射表（含持有的 writable 句柄）裹 `SendWrapper<RefCell<..>>`
//! 满足 Send+Sync。纪律：**裸 `RefCell` borrow / 裸 !Send 句柄绝不跨 await**；需要跨 await 的
//! !Send 状态一律裹进 SendWrapper——短路径在 scope 内取出 Promise 即丢、只让
//! `SendWrapper<JsFuture>` 跨 await；多步 helper（如 `opfs::open_writable`）则整段 future
//! 裹 SendWrapper。

use std::cell::RefCell;
use std::collections::HashMap;

use async_trait::async_trait;
use send_wrapper::SendWrapper;
use swarmdrop_host::{
    AppError, AppResult, FileAccess, FileSinkId, FileSourceId, FinalizedSink, HostFileMetadata,
};
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;
use web_sys::{File, FileSystemWritableFileStream, WriteCommandType, WriteParams};

use crate::opfs::{js_to_err, open_writable};

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
        // 流式直写已把全部 chunk 落盘，finalize 只需 close 句柄提交 staging、再从表中移除。
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
        // 移除即 drop writable 句柄；未 close 的 staging 写入被丢弃——正是取消/失败时该有的行为。
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
