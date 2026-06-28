## ADDED Requirements

### Requirement: Store 状态字段重构

`src/stores/transfer-store.ts` 暴露的 zustand store SHALL 维护以下字段：

- `sessions: Record<string, TransferSession>` —— 活跃会话（仅内存），key 为 session_id
- `dbHistory: MobileTransferHistoryItem[]` —— 已从 native 拉取的历史快照，每次 `loadHistory()` 后被替换
- `pendingOffers: TransferOfferQueueItem[]` —— 待用户响应的入站 offer 队列
- `lastError: string | null` —— 最近一次失败的人类可读消息

`TransferSession` 类型 SHALL 包含 session_id / direction / peerId / deviceName / files / totalSize / status / progress / error / startedAt / completedAt / saveLocation 等字段（参照桌面端 `@/lib/types#TransferSession`）。

#### Scenario: 初始状态

- **WHEN** App 冷启动、store 刚被实例化
- **THEN** `sessions === {}`、`dbHistory === []`、`pendingOffers === []`、`lastError === null`

### Requirement: loadHistory 从 native 拉取并写入 dbHistory

store SHALL 提供 `loadHistory(): Promise<void>` 方法，内部调用 `MobileCore.listTransferHistory(undefined)` 并把结果赋给 `dbHistory`。loadHistory SHALL 在以下时机被调用：

1. 应用启动后 native 准备就绪时（一次）
2. 每次 `transferComplete` / `transferFailed` / `transferPaused` / `transferResumed` 事件触发时（用于刷新历史 section）
3. 用户进入「传输历史」页面时（路由 mount）
4. 用户调用 `clearAllHistory` / `deleteHistoryItem` 之后

#### Scenario: 首次加载

- **WHEN** App 启动后 `setupTransferListeners()` 完成
- **THEN** store SHALL 立即调用 `loadHistory()` 一次

#### Scenario: 完成事件触发刷新

- **WHEN** `events.transferComplete` 事件到达
- **THEN** store SHALL 先 `await loadHistory()` 再从 `sessions` 中删除对应 session_id（避免空窗）

#### Scenario: native 抛错

- **WHEN** `MobileCore.listTransferHistory` 抛 FfiError
- **THEN** store SHALL 把 `lastError` 设为人类可读的 i18n 消息，不抛出到 UI 层、不破坏现有 `dbHistory`

### Requirement: 活跃会话事件映射

store SHALL 在 `setupTransferListeners()` 中订阅以下事件，并按规则更新 sessions / dbHistory / pendingOffers：

| 事件 | 行为 |
|------|------|
| `TransferOfferReceived` | push 到 `pendingOffers` |
| `TransferProgress` | 更新 `sessions[id].progress`，若 session 不存在则忽略 |
| `TransferAccepted` | `sessions[id].status = "transferring"` |
| `TransferRejected` | 从 `sessions` 删除 + toast 错误 |
| `TransferComplete` | `removeAndRefresh(id)` —— 先 loadHistory 再删除 sessions[id] |
| `TransferFailed` | 同 TransferComplete + toast |
| `TransferPaused` | 同 TransferComplete + toast "对方已暂停传输" |
| `TransferResumed` | `addSession(...)` + `loadHistory()` |

#### Scenario: Progress 事件按 sessionId 更新

- **GIVEN** sessions 中有 sessionId = "abc" 的活跃 session
- **WHEN** `events.transferProgress` payload.sessionId === "abc"
- **THEN** `sessions["abc"].progress` SHALL 被替换为 event payload，`status` SHALL 设为 "transferring"

#### Scenario: 完成事件不出现空窗

- **GIVEN** sessions 中有 sessionId = "abc"，UI 同时渲染了「活跃」和「历史」section
- **WHEN** `events.transferComplete` 触发
- **THEN** UI 上 SHALL 先看到 "abc" 出现在历史 section，然后才从活跃 section 消失（loadHistory 完成之前不删 sessions["abc"]）

#### Scenario: Resumed 事件接管新 session

- **WHEN** `events.transferResumed` payload 描述了新建的 session_id = "def"
- **THEN** store SHALL 调用 `addSession({ sessionId: "def", direction, peerId, ..., status: "transferring", startedAt: Date.now() })`，并 `loadHistory()`

### Requirement: 历史清理与删除 action

store SHALL 提供：

- `clearAllHistory(): Promise<void>` —— 调用 `MobileCore.clearTransferHistory()` 后 `loadHistory()`
- `deleteHistoryItem(sessionId: string): Promise<void>` —— 调用 `MobileCore.deleteTransferSession(sessionId)` 后 `loadHistory()`

两者失败时 SHALL toast 错误，但仍然继续刷新 `dbHistory`（保持 UI 与 DB 一致）。

#### Scenario: 用户清空历史

- **WHEN** UI 调用 `clearAllHistory`
- **THEN** native `clearTransferHistory` SHALL 被调用，调用成功后 `dbHistory` SHALL 变为空数组，UI 显示空状态

#### Scenario: 用户删除单条

- **WHEN** UI 对某 sessionId 调用 `deleteHistoryItem`
- **THEN** native `deleteTransferSession` SHALL 被调用，`dbHistory` SHALL 不再包含该项

### Requirement: 恢复传输 action

store SHALL 提供 `resumeHistoryItem(sessionId: string): Promise<string>`，内部调用 `MobileCore.resumeTransfer(sessionId)`，把返回的 `MobileResumeTransferResult` 转为 `TransferSession` 调用 `addSession`，再 `loadHistory()` 刷新历史；返回新建会话的 session_id。

#### Scenario: 恢复成功

- **WHEN** UI 调用 `resumeHistoryItem("abc")` 且 native 调用成功
- **THEN** store SHALL `addSession({ sessionId, direction, peerId, ..., status: "transferring" })`，dbHistory 刷新；返回 sessionId

#### Scenario: 恢复失败（对端离线）

- **WHEN** native `resumeTransfer` 抛 FfiError
- **THEN** store SHALL toast 错误，`sessions` SHALL 保持不变，`lastError` SHALL 设为对应消息；调用方 Promise 被 reject

### Requirement: setup / cleanup 生命周期

store SHALL 提供：

- `setupTransferListeners(): Promise<void>` —— 订阅所有事件 + 调用一次 loadHistory，返回前清理上次的 listener 防止重复
- `cleanupTransferListeners(): Promise<void>` —— 解绑所有 listener

setupTransferListeners SHALL 在 App 根布局（`src/app/_layout.tsx` 或其等价处）内调用，与 mobile-core 启动后的生命周期对齐。

#### Scenario: App 启动设置 listener

- **WHEN** native 节点 ready
- **THEN** `setupTransferListeners()` SHALL 被调用一次，订阅事件并初始化 dbHistory

#### Scenario: 重复 setup 不泄漏

- **WHEN** `setupTransferListeners()` 被连续调用两次
- **THEN** 第二次调用前 SHALL 清理上一次的 listener，事件不重复触发 store
