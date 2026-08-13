## Why

传输流程代码审查发现了多个高/中风险问题：断点续传前端集成缺失（恢复后无法显示进度）、offer 消费竞争、取消操作无乐观更新、资源泄漏（prepared/session 无超时清理）、以及多处时序竞态。这些问题直接影响用户体验和系统可靠性，需要在进入 Phase 4（Mobile）前修复。

## What Changes

- 修复 `resumeTransfer` 成功后不创建运行时 session 的问题，使恢复传输的进度可在活跃列表中实时展示
- 统一 offer 消费机制：移除 `ReceivePage` 的 offer 消费逻辑，仅保留全局 `TransferOfferDialog`
- 为 `startSend` 等待对方确认阶段添加"等待确认"UI 状态和取消能力
- 取消操作增加前端乐观更新（先更新 UI 状态，Rust 回调确认）
- 修复 `event_loop.rs` Cancel 处理中 `cancel()` 改为 `cancel_and_wait()` 避免 bitmap/part 竞争
- Rust 端 `receiver.rs` 解密失败纳入重试逻辑
- 添加 `prepared` DashMap、`SendSession`、`PendingOffer` 的超时清理机制
- 简化前端 history 数据源：去除内存 `history` 数组，统一从 DB 加载
- `TransferItem` 渲染优化：基于 sessionId 独立订阅 store 避免全列表重渲染
- 多处 i18n 硬编码修复

## Capabilities

### New Capabilities
- `transfer-session-cleanup`: 传输会话资源超时清理机制（prepared、send session、pending offer 的自动过期回收）

### Modified Capabilities

（无现有 spec 需要修改，所有变更为新增能力或 bug 修复）

## Impact

- **Rust 后端**: `transfer/offer.rs`（超时清理）、`transfer/receiver.rs`（解密重试）、`network/event_loop.rs`（cancel 竞态修复）、`commands/transfer.rs`（resume 返回值扩展）
- **前端 Store**: `transfer-store.ts`（去除 history 数组、乐观更新、offer 消费重构）
- **前端页面**: `send/index.lazy.tsx`（等待确认状态）、`transfer/-transfer-item.tsx`（memo + sessionId 订阅）、`transfer/index.lazy.tsx`（history 简化）、`transfer/$sessionId.lazy.tsx`（cancelled 状态处理）、`receive/index.lazy.tsx`（移除 offer 消费）
- **全局组件**: `transfer-offer-dialog.tsx`（唯一 offer 入口）
- **i18n**: 多个组件的硬编码中文字符串
