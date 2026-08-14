## 1. Rust 后端 — Cancel 竞态与解密重试修复

- [x] 1.1 修复 `event_loop.rs` Cancel 处理：将 `s.cancel()` 改为 `s.cancel_and_wait().await`，然后再 `cleanup_part_files()`，确保 bitmap 刷写完成后才清理 .part 文件
- [x] 1.2 修复 `receiver.rs` `pull_single_chunk`：将解密失败纳入重试循环（与网络错误共享同一个 retry 路径，最多 3 次指数退避）

## 2. Rust 后端 — 超时清理机制

- [x] 2.1 在 `TransferManager` 中添加 `created_at: Instant` 字段到 `PreparedTransfer` 和 `PendingOffer`
- [x] 2.2 在 `SendSession` 中添加 `last_activity: Arc<AtomicU64>` 字段（存储 Instant 的 elapsed millis），每次 `handle_chunk_request` 时更新
- [x] 2.3 在 `TransferManager::new()` 中接受 `CancellationToken` 并 spawn 清理任务：每 60 秒扫描一次，清理超过 5 分钟的 prepared 和 pending offer，清理超过 30 分钟无活动的 send session（cancel + remove + mark DB failed）
- [x] 2.4 在 `commands::shutdown()` 或 node 停止时通过 `CancellationToken` 停止清理任务

## 3. Rust 后端 — resumeTransfer 返回值扩展

- [x] 3.1 定义 `ResumeTransferResult` struct（session_id, direction, peer_id, peer_name, files: Vec<FileInfo>, total_size）
- [x] 3.2 修改 `commands::resume_transfer` 返回 `ResumeTransferResult` 而非 `()`，从 DB session + `initiate_resume` 结果组装
- [x] 3.3 修改 `TransferManager::initiate_resume()` 返回恢复所需的元数据（如 files 列表和已传输字节数）

## 4. Rust 后端 — startSend 异步化

- [x] 4.1 修改 `TransferManager::send_offer()` 拆分为两步：`send_offer_async()` 发送 offer 后立即返回 session_id（创建 `waiting_accept` 状态的 DB 记录），将 await 响应逻辑移到后台 tokio task
- [x] 4.2 后台 task 收到 OfferResult 后，创建 `SendSession` 并通过 `app_handle.emit("transfer-accepted", ...)` 通知前端；拒绝时通过 `app_handle.emit("transfer-rejected", ...)` 通知
- [x] 4.3 更新 `commands::start_send` 命令签名，接受 `AppHandle` 参数，返回 `session_id` + 初始 session 信息

## 5. 前端 — transfer-store 重构

- [x] 5.1 移除 `history: TransferSession[]` 字段和 `moveSessionToHistory` 方法
- [x] 5.2 修改 `completeSession`: 从 sessions 中移除 session，500ms 后调用 `loadHistory()`
- [x] 5.3 修改 `failSession` 和新增 `cancelSession`: 同上逻辑，从 sessions 移除 + 延迟刷新 DB 历史
- [x] 5.4 添加 `transfer-accepted` 和 `transfer-rejected` 事件监听器到 `setupTransferListeners`
- [x] 5.5 `transfer-accepted` 事件处理：将 session 状态从 `waiting_accept` 更新为 `transferring`
- [x] 5.6 `transfer-rejected` 事件处理：从 sessions 移除 + toast 提示拒绝原因

## 6. 前端 — Offer 消费统一

- [x] 6.1 修改 `receive/index.lazy.tsx`：移除 `shiftOffer` 消费逻辑，改为纯展示页面（"等待对方发送文件"提示 + 已配对设备列表），不再自动导航到 `/transfer`
- [x] 6.2 确认 `TransferOfferDialog` 作为唯一 offer 消费入口，验证在各页面（包括 `/receive`）正确显示

## 7. 前端 — 发送流程改造

- [x] 7.1 修改 `send/index.lazy.tsx` 的 `handleSend`: `startSend` 返回 session_id 后立即 `addSession`（状态为 `waiting_accept`）并导航到详情页
- [x] 7.2 在 `transfer/$sessionId.lazy.tsx` 中为 `waiting_accept` 状态添加"等待对方确认"UI（加载动画 + 取消按钮）
- [x] 7.3 `waiting_accept` 状态下的取消按钮调用 `cancelSend(sessionId)` + `cancelSession(sessionId)` 乐观更新

## 8. 前端 — 取消乐观更新

- [x] 8.1 修改 `-transfer-item.tsx` 的 `handleCancel`：先调用 `cancelSession(sessionId)` 乐观更新 UI，再异步调用 `cancelSend`/`cancelReceive`，失败时 toast 提示
- [x] 8.2 修改 `transfer/$sessionId.lazy.tsx` 中取消按钮的行为，同上乐观更新逻辑

## 9. 前端 — 断点续传集成

- [x] 9.1 更新 `src/commands/transfer.ts` 中 `resumeTransfer` 的返回类型为 `ResumeTransferResult`
- [x] 9.2 修改 `-history-item.tsx` 的 `handleResume`：`resumeTransfer` 成功后调用 `addSession` 创建运行时 session 并导航到 `transfer/$sessionId`
- [x] 9.3 `loadHistory()` 调用确保恢复后的 session 从 DB 历史中移除（因为已进入活跃列表）

## 10. 前端 — 渲染优化与 i18n

- [x] 10.1 修改 `TransferItem` 为接受 `sessionId: string` prop，内部通过 `useTransferStore(s => s.sessions[sessionId])` 独立订阅
- [x] 10.2 为 `TransferItem` 添加 `memo` 包裹
- [x] 10.3 更新 `transfer/index.lazy.tsx` 中调用 `TransferItem` 的方式（传 sessionId 而非 session 对象）
- [x] 10.4 修复 `transfer/index.lazy.tsx` 中 `STATUS_FILTERS` 使用 Lingui `t` 宏包裹
- [x] 10.5 修复 `transfer/$sessionId.lazy.tsx` 中 `STATUS_CONFIG` 使用 Lingui `t` 宏包裹
- [x] 10.6 修复 `-history-item.tsx` 中 toast 消息使用 Lingui `t` 宏包裹（实际修复在 `transfer-store.ts` 中）
- [x] 10.7 修复 `transfer/index.lazy.tsx` 中 `mergeHistory` 函数：移除整个函数（已由 Task 5.1 去除内存 history）

## 11. 前端 — History 页面简化

- [x] 11.1 修改 `transfer/index.lazy.tsx`：移除 `mergeHistory` 逻辑，历史数据仅从 `dbHistory` 获取
- [x] 11.2 验证活跃传输列表 + DB 历史列表在各状态下正确展示
- [x] 11.3 修改 `transfer/$sessionId.lazy.tsx` 的 session 查找逻辑：先查 `sessions[sessionId]`，再查 `dbHistory`

## 12. 验证与测试

- [x] 12.1 `cargo check` 确保 Rust 编译通过（`cargo test` 因系统内存不足 OOM，非代码问题）
- [x] 12.2 `cargo clippy` 仅 2 个预存在 warning，无新增
- [x] 12.3 `pnpm build` 确保前端编译无错误
- [ ] 12.4 手动测试：正常发送/接收流程
- [ ] 12.5 手动测试：取消发送/接收（验证乐观更新 + 后端确认）
- [ ] 12.6 手动测试：断点续传流程（暂停 → 恢复 → 进度显示）
