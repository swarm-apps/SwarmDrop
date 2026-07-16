## Why

移动端的 `src/app/transfer/index.tsx` 名为「传输历史」，但实际上只渲染内存里活跃的 progress 快照——一旦 `transferComplete` / `transferFailed` 事件触发，store 立刻 `removeSession` 把记录抹掉，用户根本看不到任何已结束的传输。底层 SQLite 早就在写历史（共享 `swarmdrop-core` 已封装好 `get_transfer_history` / `clear_all_history` / `mark_session_*` 等全套 ops），桌面端也基于这套数据做出了「活跃 + 历史」双 section、过滤、清空、详情、暂停/恢复的成熟体验。缺的只是 **mobile-core 的 uniffi 桥接层** 和对应的 **RN UI**。

现在补齐，可以一次性把移动端的传输体感拉到与桌面端对齐的水准，并为后续「断点续传」「跨会话排队」打好数据基础。

## What Changes

- **mobile-core Rust**：在 `packages/swarmdrop-core/rust/mobile-core/src/transfer.rs` 新增 5 个 `#[uniffi::export]` 方法：`list_transfer_history(status_filter)`、`get_transfer_session_detail(session_id)`、`delete_transfer_session(session_id)`、`clear_transfer_history()`、`resume_transfer(session_id)`，全部包一层共享 `swarmdrop-core::database::ops::*`。
- **mobile-core Rust**：新增 uniffi 类型 `MobileTransferHistoryItem` / `MobileTransferHistoryFile` / `MobileSessionStatus`（enum）/ `MobileResumeTransferResult`，并为 `entity::transfer_session::ModelEx` 写 `From` 转换，风格对齐现有 `events.rs`。
- **mobile-core Rust**：在 `MobileCore::start_node` 末尾加 **启动 reconcile**——扫描 DB 里所有 `transferring` / `pending` / `waiting_accept` 状态的 session，批量 `mark_session_failed("app_interrupted")`，清掉崩溃/被系统杀死留下的脏状态。
- **RN store**：重写 `src/stores/transfer-store.ts` 数据模型，对齐桌面端：`sessions`（活跃，内存）+ `dbHistory`（持久化，从 mobile-core 拉）+ `pendingOffers`；`transferComplete` / `transferFailed` / `transferPaused` 改为 `removeAndRefresh` —— 先 `loadHistory()` 再从活跃移除。
- **RN store**：监听 mobile-core 已有但 RN 没接的事件 `TransferPaused` / `TransferResumed` / `TransferAccepted` / `TransferRejected`，加 toast + store 同步。
- **RN UI**：重写 `src/app/transfer/index.tsx` 为「活跃传输」+「传输历史」双 section，历史 section 提供状态过滤（全部 / 已完成 / 失败 / 已暂停 / 已取消）+ 清空操作。空状态保留。
- **RN UI**：扩充 `src/app/transfer/[sessionId].tsx` 详情页：基本信息（对端、方向、时间、状态、错误）+ 文件列表（点击 `expo-sharing.shareAsync` 分享、长按复制 URI）+ 操作按钮（活跃态：暂停 / 取消；非活跃态：删除 / 重新发送/恢复）。
- **新依赖**：`expo-sharing`（分享保存的文件）+ `@bottom-tabs/react`/`@bottom-tabs/react-native` 评估（如有需要为详情页 toolbar），优先单包 `expo-sharing`。
- **国际化**：所有新文案接入 lingui，复用桌面端措辞保持术语一致。

## Capabilities

### New Capabilities

- `mobile-core-history-api`：mobile-core 通过 uniffi 暴露传输历史的查询、清理、删除、恢复接口，以及启动时的脏状态 reconcile 行为。RN 侧的所有"看到结束态传输"能力都依赖于此。
- `mobile-transfer-history-ui`：移动端「传输历史」页与「会话详情」页的 UX —— 活跃 / 历史 双视图、状态过滤、清空、详情、文件分享、恢复 / 重发 / 删除。
- `mobile-transfer-store`：RN zustand store 的事件驱动数据模型，活跃 session 与 DB 历史的合一管理、事件 → store 的映射协议。

### Modified Capabilities

无（移动端目前没有任何已存在的 spec 文件）。

## Impact

**代码**：

- `packages/swarmdrop-core/rust/mobile-core/src/transfer.rs`（新增 5 个 uniffi 方法 + 转换层）
- `packages/swarmdrop-core/rust/mobile-core/src/app.rs`（`start_node` 末尾加 reconcile 调用）
- `packages/swarmdrop-core/rust/mobile-core/src/error.rs`（如需新增 `app_interrupted` 等错误常量）
- `packages/swarmdrop-core/src/generated/swarmdrop_mobile_core.ts`（ubrn 重新生成，不需手写）
- `src/stores/transfer-store.ts`（重写数据模型与事件接入）
- `src/app/transfer/index.tsx`（重写列表页）
- `src/app/transfer/[sessionId].tsx`（扩充详情页）
- `src/components/recent-transfer-row.tsx`（若适配新模型，否则保留原有内存活跃用途）
- 新增 `src/components/history-transfer-row.tsx`（历史条目卡片）

**依赖**：

- 新增 `expo-sharing`（详情页文件分享）
- `swarmdrop-core` git 引用可能需要 bump 到包含 history reconcile 所需 ops 的新 commit

**构建/发布**：

- mobile-core Rust 改动需要重新 `ubrn build` + 重打 iOS xcframework + Android `.so`
- 因为有新增 native 接口，旧版 RN bundle 与新版 native bundle 不兼容，**不可以热更**，必须重新 build 上架。

**不在范围内**：

- 分页 / cursor —— 首版全量返回，等真实数据量出现性能问题再优化
- 多设备同步历史 —— 仅本机
- 「重新发送」的 native 实现：首版 UI 入口存在，复用现有 `prepareSend` + `sendPrepared` 流程，不引入新 native 命令
