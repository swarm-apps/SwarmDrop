//! 文件访问端口的 Web 实现（[`FileAccess`]）。
//!
//! - **发送侧源**：用户经 `<input type=file>` 选的 [`web_sys::File`] 存 `FileSourceId → File`
//!   映射；`read_source_chunk` 走 `File.slice(offset,end).arrayBuffer()` 读 range。
//! - **接收侧 sink**：**流式落盘**——`create_sink` 时开一个 OPFS `createWritable` 句柄并持有
//!   （Window / Worker 通吃），每 chunk 用 `WriteParams { position, data }` **positioned 直写**
//!   （单次 Promise 往返），`finalize_sink` 时 `close` 提交。不整文件缓冲入内存（大文件不 OOM）。
//!   曾试过 Worker 侧用 SyncAccessHandle——**实测吞吐无增益**（瓶颈在网络链路），为免双写法
//!   已移除（语义差异与坑见知识库 libp2p-wasm.md）。完成后经 [`crate::opfs::export_blob_url`]
//!   读回建 blob URL 供下载；取消 / 失败走 `cleanup_sink`——`abort` 掉句柄并**真删掉**那条
//!   OPFS 文件（Web 写的是最终路径而非 `.part`，残件长得像正常文件）。
//!   OPFS 原语（开句柄/建目录链/删条目/blob 导出）在 [`crate::opfs`]。
//!
//! JsValue `!Send`，而 [`FileAccess`] 是 `Send`：用 `send_wrapper::SendWrapper` 兜 Send（单线程
//! wasm 永不触发其跨线程 panic）。映射表（含持有的 writable 句柄）裹 `SendWrapper<RefCell<..>>`
//! 满足 Send+Sync。纪律：**裸 `RefCell` borrow / 裸 !Send 句柄绝不跨 await**；需要跨 await 的
//! !Send 状态一律裹进 SendWrapper——短路径在 scope 内取出 Promise 即丢、只让
//! `SendWrapper<JsFuture>` 跨 await；多步 helper（如 `opfs::open_writable`）则整段 future
//! 裹 SendWrapper。

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};

use async_trait::async_trait;
use send_wrapper::SendWrapper;
use swarmdrop_host::{
    AppError, AppResult, FileAccess, FileSinkId, FileSourceId, FinalizedSink, HostFileMetadata,
};
use tracing::warn;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;
use web_sys::{File, FileSystemWritableFileStream, WriteCommandType, WriteParams};

use crate::opfs::{js_to_err, open_writable, remove_path};

/// 一条已登记的发送源。
///
/// **`relative_path` 必须单独存，不能从 [`FileSourceId`] 反推。** 两者曾经是同一个字符串
/// （id 就是文件名），代价是同名文件互相覆盖：`sources` 是个 map，一次发送里挑两个
/// `report.pdf`（不同目录下极常见）后者直接顶掉前者，两条 entry 于是指向同一个 `File`
/// 却各自声明着不同的 size —— 轻则 `prepare` 报「read_source_chunk 返回长度异常」，
/// 重则**发出错误的文件内容**。现在 id 是每次登记唯一的不透明值，路径归这里。
struct SourceEntry {
    file: File,
    /// 发送方声明给对端的路径。将来支持选目录时它才会真的带目录段，现在等于文件名。
    relative_path: String,
}

/// 发送源保留多少**批**（一次 `send_files` 是一批）。
///
/// 32 远超任何真实的单页会话——它的作用是把一个「只增不减」的结构变成有界的，而不是去逼近
/// 某个真实上限。见 [`OpfsFileAccess::register_batch`] 的推导。
const MAX_SOURCE_BATCHES: usize = 32;

/// OPFS + File 源的 [`FileAccess`] 实现。
pub struct OpfsFileAccess {
    sources: SendWrapper<RefCell<HashMap<FileSourceId, SourceEntry>>>,
    /// 各批的 id 清单，按登记先后。只为按批淘汰而存——见 [`Self::register_batch`]。
    source_batches: SendWrapper<RefCell<VecDeque<Vec<FileSourceId>>>>,
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
            source_batches: SendWrapper::new(RefCell::new(VecDeque::new())),
            sinks: SendWrapper::new(RefCell::new(HashMap::new())),
        }
    }

    /// 登记一批发送源（用户这一次选中的 File 集合）。
    ///
    /// `entries` 的 id 由调用方保证**每次登记唯一**（见 [`SourceEntry`]），`relative_path`
    /// 是要告诉对端的路径。
    ///
    /// # 为什么按「批」登记而不是逐条
    ///
    /// 源登记后**不能随发送结束就删**：同一页面生命周期内的续传要靠它把 `File` 找回来
    /// （浏览器不允许在用户未重新选择的前提下再读同一个文件，所以这份表就是唯一的来源）。
    /// 于是它只增不减——而 id 从「文件名」换成「每次唯一」之后，重复发送同一批文件不再复用
    /// key，长会话里会线性堆积，每条钉住一个 OS 文件引用。
    ///
    /// 上限因此按**批**算：淘汰整批，绝不淘汰半批（半批会让一次续传读到一半才发现源没了）。
    /// [`MAX_SOURCE_BATCHES`] 之外的旧批被丢弃，其后果与「刷新页面后源全没了」完全同类——
    /// 那是这份表本来就有的语义，用户见到的是同一句「文件源不存在」。
    pub fn register_batch(&self, entries: Vec<(FileSourceId, String, File)>) {
        let ids: Vec<FileSourceId> = entries.iter().map(|(id, _, _)| id.clone()).collect();
        {
            let mut sources = self.sources.borrow_mut();
            for (id, relative_path, file) in entries {
                sources.insert(
                    id,
                    SourceEntry {
                        file,
                        relative_path,
                    },
                );
            }
        }

        let mut batches = self.source_batches.borrow_mut();
        batches.push_back(ids);
        while batches.len() > MAX_SOURCE_BATCHES {
            if let Some(evicted) = batches.pop_front() {
                let mut sources = self.sources.borrow_mut();
                for id in evicted {
                    sources.remove(&id);
                }
            }
        }
    }
}

#[async_trait]
impl FileAccess for OpfsFileAccess {
    async fn source_metadata(&self, source: &FileSourceId) -> AppResult<HostFileMetadata> {
        let (file, relative_path) = self.source(source)?;
        Ok(HostFileMetadata {
            name: file.name(),
            relative_path,
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
            let (file, _) = self.source(source)?;
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
        // （positioned write 只覆盖后续 range）。**跨页面刷新的接收续传也走这条**——OPFS 里的
        // 部分文件与 checkpoint 都持久（#81），刷新后新开的句柄接着上次的字节写。
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

    /// 丢弃一条未最终化的 sink：放弃 staging 写入 **并真删掉 OPFS 里的那条文件**。
    ///
    /// 端口的默认实现是 no-op，Web 曾经继承那个语义（只 drop 句柄），于是每一次取消 /
    /// 失败的接收都在 OPFS 留下残件。而 Web 比桌面更糟的一点是**没有 `.part` 中间态**：
    /// [`create_sink`](Self::create_sink) / [`open_or_create_sink`](Self::open_or_create_sink)
    /// 直接开 `relative_path` 的写句柄，写的就是最终路径——残件是个**文件名正确、内容截断**
    /// 的东西，桌面留下的至少还叫 `xxx.part` 一眼能看出没写完。这正是它必须被删掉的理由。
    ///
    /// 顺序不可调换，见下方注释；删除失败只告警不上抛（清理不该把取消流程一起拖失败）。
    ///
    /// **只删没写完的那个**：调用方 `ReceiverActor::cleanup_part_files` 遍历的是
    /// `created_sinks`，而 `finalize_sink` 成功后该 sink 立刻被 `remove_created_sink` 摘掉，
    /// 所以已提交的文件不在清单里。这条不变量此前被 no-op 掩盖着从没被检验过，现在它决定
    /// 会不会误删用户文件——`finalized_file_survives_sibling_cleanup` 钉住它。
    async fn cleanup_sink(&self, sink: &FileSinkId) -> AppResult<()> {
        // `createWritable()` 持有该文件的**独占锁**，锁只在 close()/abort() 时释放——只把句柄
        // 从表里 drop 掉要等 GC，时机不确定，锁没放就 remove_entry 会撞
        // NoModificationAllowedError。所以先取出句柄 → abort → await → 再删条目。
        //
        // **必须是 abort() 不是 close()**：close 会把 staging 里的写入提交上去，
        // 正好与「丢弃半成品」相反。
        //
        // 裸 Promise 是 !Send，取出来即刻裹进 SendWrapper——否则它以 Option 的形态活到 await
        // 之后，整个 future 就不满足端口的 Send 了。
        let aborting = self
            .sinks
            .borrow_mut()
            .remove(sink)
            .map(|writable| SendWrapper::new(JsFuture::from(writable.abort())));
        if let Some(aborting) = aborting
            && let Err(e) = aborting.await
        {
            // 不阻断删除：锁真没放的话下一步会再报一次，那条消息更贴近后果。
            warn!("放弃 OPFS 写句柄失败: sink={}, {}", sink.0, js_to_err(e));
        }
        if let Err(e) = SendWrapper::new(remove_path(&sink.0)).await {
            warn!("删除 OPFS 半成品失败: sink={}, {}", sink.0, e);
        }
        Ok(())
    }

    /// 删除一个已最终化的文件。`uri` 是 [`finalize_sink`](Self::finalize_sink) 产出的
    /// `opfs:/{relative_path}`，而 OPFS 的键是**不带前缀**的 `relative_path`——所以这里
    /// 要剥掉它。
    ///
    /// **「哪个字段是删除键」由实现方回答，正是这条方法存在的理由。** 上层编排
    /// （`swarmdrop_transfer::inbox::delete_inbox_item`）统一递 `local_path`，桌面拿到的是
    /// 文件系统绝对路径、移动是 `file://` 或 SAF URI、这里是带前缀的 OPFS 键——三种解释
    /// 各自留在各自的实现里，编排一行 `cfg` 都不需要。
    ///
    /// 兼容不带前缀的输入：老数据或调用方直接递 `relative_path` 时照样删得掉。
    async fn delete_finalized_file(&self, uri: &str) -> AppResult<()> {
        let key = uri.strip_prefix("opfs:/").unwrap_or(uri);
        SendWrapper::new(remove_path(key)).await?;
        Ok(())
    }
}

impl OpfsFileAccess {
    /// 查表取源（`File` 的 clone 只是 wasm-bindgen 堆表引用计数，非数据拷贝）。
    fn source(&self, source: &FileSourceId) -> AppResult<(File, String)> {
        self.sources
            .borrow()
            .get(source)
            .map(|entry| (entry.file.clone(), entry.relative_path.clone()))
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

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

    fn metadata(relative_path: &str) -> HostFileMetadata {
        HostFileMetadata {
            name: relative_path
                .rsplit('/')
                .next()
                .unwrap_or(relative_path)
                .into(),
            relative_path: relative_path.into(),
            size: 8,
            modified_at: None,
            checksum: None,
            save_dir: None,
        }
    }

    /// 取消一条多文件接收会话时，**已提交的文件不能被连坐删掉**。
    ///
    /// 域层的保证是 `ReceiverActor` 在 `finalize_sink` 成功后立刻 `remove_created_sink`，
    /// 于是 `cleanup_part_files` 的清单里只剩没写完的那些。这里按同样的顺序走一遍：
    /// A 写完并 finalize、B 只写了一半，只对 B 调 `cleanup_sink` —— A 必须还在。
    ///
    /// `cleanup_sink` 变成真删之前这条无从检验（no-op 掩盖一切），而它现在决定的是
    /// 会不会误删用户文件。
    #[wasm_bindgen_test]
    async fn finalized_file_survives_sibling_cleanup() {
        let access = OpfsFileAccess::new();
        let done = "cleanup-sink-test/done.bin";
        let partial = "cleanup-sink-test/partial.bin";

        let done_sink = access.create_sink(metadata(done)).await.expect("开 A");
        access
            .write_sink_chunk(&done_sink, 0, vec![1, 2, 3, 4])
            .await
            .expect("写 A");
        access.finalize_sink(&done_sink).await.expect("提交 A");

        let partial_sink = access.create_sink(metadata(partial)).await.expect("开 B");
        access
            .write_sink_chunk(&partial_sink, 0, vec![9; 4])
            .await
            .expect("写 B");

        access.cleanup_sink(&partial_sink).await.expect("清理 B");

        assert!(
            crate::opfs::export_blob_url(done).await.is_ok(),
            "已提交的文件不该被取消连坐删掉"
        );
        assert!(
            crate::opfs::export_blob_url(partial).await.is_err(),
            "半成品应当已被删掉"
        );

        let _ = crate::opfs::remove_path(done).await;
    }

    /// 建一个内容为 `bytes` 的 `File`，文件名 `name`。
    fn file_of(name: &str, bytes: &[u8]) -> File {
        let parts = js_sys::Array::new();
        parts.push(&js_sys::Uint8Array::from(bytes).into());
        File::new_with_u8_array_sequence(&parts, name).expect("建 File")
    }

    /// **同名文件不能互相覆盖。**
    ///
    /// 源 id 曾经就是文件名，于是一次发送里挑两个 `report.pdf` 时后者顶掉前者，两条 entry
    /// 指向同一个 `File`——发出去的内容是错的，而这件事在发送侧没有任何报错（接收端验签才
    /// 暴露，且归因指向网络）。这条测试钉住「id 与 relative_path 是两件事」。
    #[wasm_bindgen_test]
    async fn same_name_sources_do_not_collide() {
        let access = OpfsFileAccess::new();
        let a = FileSourceId("batch/0".into());
        let b = FileSourceId("batch/1".into());

        access.register_batch(vec![
            (
                a.clone(),
                "report.pdf".into(),
                file_of("report.pdf", b"AAAA"),
            ),
            (
                b.clone(),
                "report.pdf".into(),
                file_of("report.pdf", b"BBBBBB"),
            ),
        ]);

        // 读到的必须是各自那个 File 的内容——这才是 bug 的真身。
        assert_eq!(
            access.read_source_chunk(&a, 0, 4).await.expect("读 A"),
            b"AAAA",
            "同一批里的第二个同名文件不该改写第一条源"
        );
        assert_eq!(
            access.read_source_chunk(&b, 0, 6).await.expect("读 B"),
            b"BBBBBB"
        );
    }

    /// 源表只增不减，所以按**批**设了上限。这条钉住两件事：超限的旧批真的被丢掉，
    /// 且淘汰是**整批**——半批淘汰会让一次续传读到一半才发现源没了。
    #[wasm_bindgen_test]
    async fn oldest_source_batches_are_evicted_whole() {
        let access = OpfsFileAccess::new();
        let id = |batch: usize, idx: usize| FileSourceId(format!("b{batch}/{idx}"));

        for batch in 0..=MAX_SOURCE_BATCHES {
            access.register_batch(vec![
                (id(batch, 0), "a.bin".into(), file_of("a.bin", b"xx")),
                (id(batch, 1), "b.bin".into(), file_of("b.bin", b"yy")),
            ]);
        }

        // 第 0 批整批被淘汰——两条都不在了，不是只掉一条。
        assert!(access.read_source_chunk(&id(0, 0), 0, 2).await.is_err());
        assert!(access.read_source_chunk(&id(0, 1), 0, 2).await.is_err());
        // 最新那批完好。
        assert_eq!(
            access
                .read_source_chunk(&id(MAX_SOURCE_BATCHES, 0), 0, 2)
                .await
                .expect("最新批必须还在"),
            b"xx"
        );
    }
}
