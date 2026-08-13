## 1. 数据库初始化

- [x] 1.1 创建 `src-tauri/src/database/mod.rs` 模块：初始化 SeaORM DatabaseConnection（SQLite），执行 migration，返回连接
- [x] 1.2 在 `lib.rs` 的 setup() 中调用数据库初始化，将 DatabaseConnection 注入 Tauri managed state
- [x] 1.3 添加 `sea-orm`/`sea-orm-migration` 依赖到 `src-tauri/Cargo.toml`（若尚未添加），确保 `entity` 和 `migration` crate 可被主 crate 引用

## 2. 传输记录持久化

- [x] 2.1 在 `transfer/offer.rs` 中：接收方接受 Offer 时创建 transfer_session + transfer_files 记录（direction=receive, status=transferring, bitmap 全零）
- [x] 2.2 在 `network/event_loop.rs` 中：发送方收到 OfferResult(accepted) 时创建 transfer_session + transfer_files 记录（direction=send, status=transferring）
- [x] 2.3 在 `transfer/receiver.rs` 中：每 10 个 chunk 执行一次 bitmap + transferred_bytes 的 DB UPDATE
- [x] 2.4 在传输完成路径中：更新 session status=completed, finished_at, 文件 status=completed
- [x] 2.5 在传输失败路径中：更新 session status=failed, error_message, finished_at，接收方清空 bitmap + 删除 .part
- [x] 2.6 在取消路径中：更新双方 session status=cancelled, finished_at，接收方清空 bitmap + 删除 .part

## 3. 启动清理逻辑

- [x] 3.1 实现启动清理函数：sender transferring → failed
- [x] 3.2 实现启动清理函数：receiver transferring → 根据 bitmap 状态判断 paused/failed/completed
- [x] 3.3 实现启动清理函数：receiver paused 超过 7 天 → failed，清理 .part 文件
- [x] 3.4 在 setup() 中 DB 初始化后调用启动清理函数

## 4. 断点续传协议

- [x] 4.1 在 `protocol.rs` 中添加 `FileChecksum` 结构体、`ResumeRejectReason` 枚举、`TransferRequest::ResumeRequest` 和 `TransferResponse::ResumeResult` 变体
- [x] 4.2 在 `transfer/sender.rs` 中实现 ResumeRequest 处理：查 DB、验证文件校验和、回复 Accept/Reject
- [x] 4.3 在 `transfer/receiver.rs` 中实现 resume 发起：读 DB bitmap、构造 ResumeRequest、处理 ResumeResult
- [x] 4.4 在 `network/event_loop.rs` 中添加 ResumeRequest/ResumeResult 消息路由

## 5. 暂停传输

- [x] 5.1 在 `transfer/receiver.rs` 中实现暂停逻辑：停止 ChunkRequest、刷写 bitmap、更新 DB status=paused、断开连接
- [x] 5.2 在 `transfer/sender.rs` 中实现 peer_paused 运行时状态检测和前端事件通知
- [x] 5.3 在 `commands/transfer.rs` 中实现 `pause_transfer` command 并注册到 lib.rs

## 6. 传输历史 API

- [x] 6.1 在 `commands/transfer.rs` 中实现 `get_transfer_history` command（支持可选 status 过滤，返回 sessions + files）
- [x] 6.2 在 `commands/transfer.rs` 中实现 `get_transfer_session` command（按 session_id 查询，含关联 files）
- [x] 6.3 在 `commands/transfer.rs` 中实现 `delete_transfer_session` command（删除 session + files，清理 .part）
- [x] 6.4 在 `commands/transfer.rs` 中实现 `clear_transfer_history` command（清空所有记录 + .part）
- [x] 6.5 在 `commands/transfer.rs` 中实现 `resume_transfer` command（触发 ResumeRequest 流程）
- [x] 6.6 将所有新 command 注册到 `lib.rs` 的 `generate_handler!`

## 7. 前端集成

- [x] 7.1 在 `src/commands/transfer.ts` 中添加新 command 的 TypeScript wrappers（get_transfer_history, delete_transfer_session, clear_transfer_history, resume_transfer, pause_transfer）
- [x] 7.2 更新 `src/stores/transfer-store.ts`：setupTransferListeners 中调用 get_transfer_history 加载历史
- [x] 7.3 更新 `src/stores/transfer-store.ts`：传输完成/失败/取消事件后刷新历史
- [x] 7.4 更新传输列表页：展示持久化历史，paused 会话显示"恢复"按钮，支持按状态过滤
- [x] 7.5 实现历史记录删除 UI（单条删除 + 清空所有）

## 8. 验证与测试

- [x] 8.1 确保 `cargo build` 编译通过
- [x] 8.2 确保 `cargo clippy` 无 warning
- [ ] 8.3 手动测试：完整传输流程（发送 → 接收 → 历史记录可见）
- [ ] 8.4 手动测试：暂停 → 恢复 → 传输继续
- [ ] 8.5 手动测试：app 重启后历史记录仍在，paused 会话可恢复
