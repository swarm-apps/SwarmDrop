## Context

SwarmDrop 的文件传输使用拉取式架构——接收方（ReceiveSession）主动请求分块，发送方（SendSession）被动响应。当前只有接收方集成了 `ProgressTracker`，发送方仅通过 `AtomicU64` 累加 `bytes_sent` 但从不发射进度事件。此外 `TransferProgressEvent` 仅追踪 `current_file`（单个文件）和 `completed_files`（计数），没有每文件的独立状态。

关键约束：
- `SendSession.handle_chunk_request()` 在 event_loop 中被 spawn 为独立 tokio 任务并发执行，ProgressTracker 需要线程安全共享
- 现有的 200ms 节流和 3 秒滑动窗口机制已经很好，不需要重写
- 前端 `TransferProgressEvent` 类型变更需要前后端同步

## Goals / Non-Goals

**Goals:**
- 发送方 UI 展示实时进度（速度、ETA、百分比、当前文件）
- 每个文件展示独立状态（pending / transferring / completed）和已传输字节数
- 复用现有 ProgressTracker 基础设施（滑动窗口、节流、事件发射）

**Non-Goals:**
- 不改变拉取式协议架构
- 不改变事件名称或全局广播模式（`app.emit("transfer-progress")` 保持不变，前端已有 sessionId 过滤）
- 不做持久化进度（重启后清空）
- 不改变分块大小或并发度

## Decisions

### 1. SendSession 集成 ProgressTracker 的方式

**选择：`Arc<std::sync::Mutex<ProgressTracker>>`**

发送方的 `handle_chunk_request` 由 event_loop 中的 `tokio::spawn` 并发调用。ProgressTracker 需要 `&mut self`（`add_bytes`、`emit_progress`）。

选择 `std::sync::Mutex` 而非 `tokio::Mutex`——因为锁内只做 `add_bytes`（VecDeque push/pop）+ `emit_progress`（序列化 + emit），不涉及 `.await`，持有时间极短（微秒级）。

- 替代方案 A：`tokio::Mutex` — 不必要的调度开销
- 替代方案 B：原子变量 + 定时器 — 需要额外 spawn 定时任务，复杂度高

### 2. 发送方如何追踪文件完成

**选择：通过 `is_last` 标志在 `handle_chunk_request` 中检测文件最后一个分块**

`handle_chunk_request` 已经计算了 `is_last = chunk_index + 1 >= total_chunks`。当 `is_last == true` 时，可以标记该文件为 completed。但由于并发乱序，不能简单用 `is_last`——需要用 per-file 的分块完成计数（AtomicU32）来判断。

**选择：ProgressTracker 内部维护 `Vec<FileProgressInfo>`，每个文件有 `chunks_done: u32` 和 `total_chunks: u32`**

当 `chunks_done == total_chunks` 时标记文件为 completed。这样即使分块乱序到达也能正确判断。

### 3. TransferProgressEvent 扩展方式

**选择：新增 `files: Vec<FileProgressInfo>` 字段**

```rust
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileProgressInfo {
    pub file_id: u32,
    pub name: String,
    pub size: u64,
    pub transferred: u64,
    pub status: &'static str,  // "pending" | "transferring" | "completed"
}
```

保留现有的 `current_file` 字段不动（向后兼容），新增 `files` 字段。接收方的 ProgressTracker 也同步维护这个列表。

### 4. 发送方的 ProgressTracker 初始化时机

**选择：在 `SendSession::new()` 中初始化**

`new()` 已经拿到 `files: Vec<PreparedFile>`，可以计算 `total_size` 和构建初始 `files` 列表。不需要延迟初始化。

### 5. event_loop 中的进度上报

**选择：在 `handle_chunk_request` 返回后、send_response 之前，由 SendSession 内部上报**

`handle_chunk_request` 内部已持有 `AppHandle`，在加密完成后调用 `self.progress.lock().add_bytes() + emit_progress()`。不需要修改 event_loop。

## Risks / Trade-offs

- **[发送方锁竞争]** → 8 路并发 ChunkRequest 都需要锁 ProgressTracker。但锁内操作是微秒级的 VecDeque 操作 + 200ms 节流（大部分调用直接 return），实际竞争极低
- **[事件体积增加]** → `files` 数组在大量文件传输时可能使事件体积增大。但 200ms 节流 + files 只包含 id/name/size/transferred/status 五个字段，即使 1000 个文件也只有几十 KB → 可接受
- **[向后兼容]** → 前端代码需要同步更新类型定义。但 SwarmDrop 是自包含应用，前后端一起发布，不存在版本不一致的问题
