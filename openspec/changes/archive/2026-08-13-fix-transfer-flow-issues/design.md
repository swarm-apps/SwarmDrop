## Context

SwarmDrop 的传输模块已经完成了核心功能（发送/接收/加密/断点续传），但代码审查发现了多个影响可靠性和用户体验的问题。当前传输流程涉及 Rust 后端��sender/receiver/offer/event_loop/progress/crypto）和前端（transfer-store/transfer pages/offer dialog）两层，问题横跨前后端。

主要问题集中在：
1. 前端断点续传集成不完整（恢复后无运行时 session）
2. Offer 消费架构不一致（Dialog vs Page 竞争）
3. 内存资源无超时回收
4. 取消/错误处理时序不严谨

## Goals / Non-Goals

**Goals:**
- 修复断点续传前端集成，使恢复传输可在活跃列表中实时展示进度
- 消除 offer 消费竞争，统一为全局 Dialog 模式
- 为所有内存中的临时资源（prepared、send session、pending offer）添加超时清理
- 修复 cancel 事件中的 bitmap/part 竞争条件
- 解密失败纳入重试路径
- 改善发送等待体验（可取消 + 状态提示）
- 取消操作前端乐观更新
- 简化 history 数据架构
- 优化 TransferItem 渲染性能
- 修复 i18n 硬编码

**Non-Goals:**
- 不重构加密方案（当前 XChaCha20-Poly1305 设计规范）
- 不改变 libp2p 传输层架构
- 不添加暂停 UI 入口（暂停功能完整性留到后续迭代）
- 不处理 Android 端 FileSource 兼容性（Phase 4 范围）
- 不添加密钥 Zeroize（桌面端风险可接受）

## Decisions

### D1: resumeTransfer 返回完整 session 元数据

**选择**: `resume_transfer` Rust 命令返回 `TransferSessionInfo`（含 session_id, direction, peer_name, files, total_size 等），前端据此调用 `addSession` 创建运行时 session。

**替代方案**: Rust 端通过事件通知前端创建 session —— 增加了事件类型和时序复杂度，不如命令返回值直接。

### D2: 统一 Offer 为全局 Dialog 模式

**选择**: 移除 `ReceivePage` 对 `pendingOffers` 的消费逻辑，`/receive` 路由改为纯信息展示页面（"等待接收中"）。`TransferOfferDialog` 作为唯一的 offer 处理入口。

**替代方案**: 引入 offer 分配器（根据当前路由决定谁消费）—— 过度工程化，全局 Dialog 已经覆盖所有场景。

### D3: 超时清理使用 tokio interval task

**选择**: 在 `TransferManager::new()` 中 spawn 一个定时清理任务（每 60 秒检查一次），清理超过 5 分钟的 prepared、超过 30 分钟无活动的 send session、超过 5 分钟的 pending offer。使用 `CancellationToken` 在 shutdown 时优雅退出。

**替代方案**: 惰性清理（每次访问时检查）—— 不能保证及时释放内存，且访问模式不可预测。

### D4: 去除内存 history 数组

**选择**: 前端 `transfer-store` 中去除 `history: TransferSession[]`，传输完成/失败/取消后直接调用 `loadHistory()` 从 DB 刷新。`mergeHistory` 函数不再需要。

**替代方案**: 保留双层但加 dedup 和 size limit —— 增加了不必要的复杂度，DB 查询已足够快。

### D5: 取消乐观更新策略

**选择**: `handleCancel` 先调用 `cancelSession(sessionId)` 将 session 移入 history（状态为 cancelled），同时异步调用 Rust cancel 命令。Rust 端的 `TRANSFER_FAILED` 事件到达时，因 session 已不在活跃列表中会被安全忽略。如果 Rust cancel 失败，toast 提示用户。

**替代方案**: 引入 `cancelling` 中间状态 —— 增加状态数量，且实际上 cancel 命令几乎不会失败。

### D6: startSend 拆分为异步流程

**选择**: `start_send` Rust 命令改为立即创建 `waiting_accept` 状态的 session 并返回 session_id。对方响应通过新增 `transfer-accepted` / `transfer-rejected` 事件通知前端。前端在等待期间可调用 `cancel_send` 取消。

**替代方案**: 保持阻塞但添加 AbortController —— Tauri invoke 不支持原生取消，需要额外的 cancel 命令配合，不如直接拆分清晰。

## Risks / Trade-offs

- **[Risk] resumeTransfer 返回值增大** → 只返回元数据不含文件内容，数据量可控（<1KB）
- **[Risk] 超时清理可能清理正在使用的 prepared** → 5 分钟超时足够长，用户不太可能准备 5 分钟后才发送。如果确实发生，用户重新选文件即可
- **[Risk] 去除内存 history 后完成事件到 DB 查询之间有短暂延迟** → 在 `completeSession` 中添加 500ms 延迟后再 loadHistory，确保 DB 写入已完成
- **[Risk] startSend 拆分增加事件类型** → 只增加两个事件（accepted/rejected），复杂度可控
- **[Risk] 乐观更新后 Rust 端 cancel 失败** → 极端场景，通过 toast 提示用户重试
