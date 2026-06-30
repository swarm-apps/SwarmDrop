## 1. 协议扩展

- [x] 1.1 在 `protocol.rs` 的 `TransferRequest` 中添加 `ChunkRequest`、`Complete`、`Cancel` 变体
- [x] 1.2 在 `protocol.rs` 的 `TransferResponse` 中添加 `Chunk`、`Ack` 变体
- [x] 1.3 为 `Chunk.data`（`Vec<u8>`）添加 serde bytes 序列化优化（CBOR 中 Vec<u8> 默认为数组而非 bytes）

## 2. 发送方核心

- [x] 2.1 新建 `transfer/sender.rs`：`SendSession` 结构体，持有 session_id、PreparedTransfer（含文件绝对路径）、TransferCrypto、CancellationToken
- [x] 2.2 实现 `SendSession::handle_chunk_request()`：在 spawn_blocking 中读取文件分块 → 加密 → 返回 Chunk 响应
- [x] 2.3 实现 `SendSession::handle_complete()`：清理会话资源
- [x] 2.4 实现 `SendSession::handle_cancel()`：通过 CancellationToken 取消，清理资源

## 3. 接收方核心

- [x] 3.1 新建 `transfer/receiver.rs`：`ReceiveSession` 结构体，持有 session_id、files 元信息、save_path、TransferCrypto、CancellationToken
- [x] 3.2 实现文件写入器：创建 `.part` 临时文件，按 offset 写入解密后的分块数据（spawn_blocking）
- [x] 3.3 实现 `ReceiveSession::start_pulling()`：spawn 后台 tokio 任务，使用 Semaphore(8) 并发拉取分块
- [x] 3.4 实现分块级重试：单个 ChunkRequest 失败时指数退避重试（500ms/1s/2s，最多 3 次）
- [x] 3.5 实现文件校验：所有分块完成后在 spawn_blocking 中计算 BLAKE3，与 Offer 中的 checksum 比对
- [x] 3.6 实现文件完成处理：校验通过后 `.part` 重命名为最终文件，根据 relative_path 重建目录结构
- [x] 3.7 实现传输完成流程：所有文件校验通过后发送 `Complete` 消息给发送方
- [x] 3.8 实现取消逻辑：CancellationToken 取消所有并发拉取，发送 Cancel 消息，删除 `.part` 文件

## 4. 进度追踪

- [x] 4.1 新建 `transfer/progress.rs`：`ProgressTracker` 结构体，含滑动窗口速度计算（3 秒窗口）
- [x] 4.2 实现 200ms 节流的 `emit_progress()` 方法，通过 `app.emit("transfer-progress", ...)` 推送前端
- [x] 4.3 实现 `transfer-complete` 和 `transfer-failed` 事件发射

## 5. TransferManager 整合

- [x] 5.1 重构 `transfer/offer.rs` → 扩展 OfferManager 为 TransferManager，新增 send_sessions / receive_sessions（DashMap）
- [x] 5.2 实现 `start_transfer()`：Offer 被接受后，创建 SendSession 并缓存到 send_sessions
- [x] 5.3 实现 `accept_and_start_receive()`：accept 回复后创建 ReceiveSession，启动后台拉取任务
- [x] 5.4 实现 `cancel_send()` / `cancel_receive()` 方法

## 6. 事件循环集成

- [x] 6.1 在 `event_loop.rs` 中处理 `TransferRequest::ChunkRequest`：路由到 SendSession
- [x] 6.2 在 `event_loop.rs` 中处理 `TransferRequest::Complete`：路由到 SendSession
- [x] 6.3 在 `event_loop.rs` 中处理 `TransferRequest::Cancel`：路由到对应 session，发射事件

## 7. 命令层更新

- [x] 7.1 修改 `commands/transfer.rs` 的 `accept_receive`：启用 save_path，委托 TransferManager 启动接收
- [x] 7.2 实现 `cancel_send` / `cancel_receive` 命令处理函数
- [x] 7.3 在 `lib.rs` 的 `generate_handler!` 中注册 `cancel_send`、`cancel_receive`

## 8. 编译验证

- [x] 8.1 `cargo build` 编译通过
- [x] 8.2 `cargo test` 所有测试通过（含已有 crypto 测试）
- [x] 8.3 `cargo clippy` 无警告
