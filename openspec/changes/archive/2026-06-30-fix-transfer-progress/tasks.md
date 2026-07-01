## 1. ProgressTracker 扩展：per-file 状态

- [x] 1.1 在 `progress.rs` 中新增 `FileProgressInfo` 结构体（file_id, name, size, transferred, status）
- [x] 1.2 给 `ProgressTracker` 增加 `files: Vec<FileProgressInfo>` 字段，在 `new()` 中从文件列表初始化（全部 pending）
- [x] 1.3 新增 `ProgressTracker::init_files()` 方法，接受文件列表并构建初始 `files` 状态
- [x] 1.4 新增 `ProgressTracker::update_file_chunk()` 方法，按 file_id 累加 transferred 字节、更新 chunks_done，当 chunks_done == total_chunks 时自动标记 completed
- [x] 1.5 在 `TransferProgressEvent` 中增加 `files: Vec<FileProgressInfo>` 字段
- [x] 1.6 在 `emit_progress()` 中将 `self.files.clone()` 填入事件

## 2. 接收方适配：ReceiveSession 使用 per-file 追踪

- [x] 2.1 在 `ReceiveSession::run_transfer()` 中，创建 ProgressTracker 后调用 `init_files()` 初始化文件列表
- [x] 2.2 在 `pull_file_chunks` 的分块完成回调中，调用 `update_file_chunk(file_id, chunk_size)` 替代现有的 `update_current_chunks(done, transferred)`
- [x] 2.3 在文件开始传输时调用新方法将文件状态设为 "transferring"
- [x] 2.4 验证现有的 `complete_file()` 和 `set_current_file()` 与新的 per-file 追踪一致

## 3. 发送方进度：SendSession 集成 ProgressTracker

- [x] 3.1 给 `SendSession` 增加 `progress: Arc<std::sync::Mutex<ProgressTracker>>` 字段和 `app: AppHandle` 引用
- [x] 3.2 在 `SendSession::new()` 中初始化 ProgressTracker（direction="send"，total_bytes 从文件列表计算）并调用 `init_files()`
- [x] 3.3 在 `handle_chunk_request()` 成功加密后，lock progress 调用 `add_bytes()` + `update_file_chunk()` + `emit_progress()`
- [x] 3.4 确保 SendSession 已持有 `AppHandle`（当前已有 `app` 字段，确认可用）

## 4. 前端类型同步

- [x] 4.1 在 `src/commands/transfer.ts` 的 `TransferProgressEvent` 接口中增加 `files: FileProgressInfo[]` 字段
- [x] 4.2 新增 `FileProgressInfo` TypeScript 接口（fileId, name, size, transferred, status）
- [x] 4.3 在 `src/stores/transfer-store.ts` 的 `TransferSession` 中确保 `progress` 字段能承载新的 files 数组

## 5. 传输详情页 UI 增强

- [x] 5.1 在 `$sessionId.lazy.tsx` 的文件列表中，从 `session.progress.files` 读取每个文件的状态和进度
- [x] 5.2 为 pending / transferring / completed 三种状态渲染不同的视觉指示（图标或颜色）
- [x] 5.3 为 transferring 状态的文件显示已传输字节或百分比

## 6. 验证

- [x] 6.1 `cargo build` 编译通过
- [x] 6.2 `cargo clippy` 无警告
- [x] 6.3 `cargo test` 通过（含 progress 模块已有测试）
- [x] 6.4 `pnpm build` 前端编译通过
