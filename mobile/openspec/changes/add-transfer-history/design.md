## Context

**当前状态**：

- 移动端 `MobileCore` (uniffi `#[uniffi::export]` 接口) 暴露的传输相关方法仅有 6 个：`prepare_send`、`send_prepared`、`accept_receive`、`reject_receive`、`cancel_transfer`、`pause_transfer`。**没有任何「读历史」「清空历史」「恢复传输」入口**。
- 但 `MobileCore::app` 已经持有 `Mutex<Option<Arc<sea_orm::DatabaseConnection>>>`，启动时跑 `Migrator::up`，传输服务内部也调用了共享 `swarmdrop-core` 的 `mark_session_completed` / `mark_session_failed` 等写入函数 —— **SQLite 表里此刻已经有真实的历史数据，只是 RN 这边没有任何方式读出来**。
- 共享 crate `swarmdrop-core::database::ops`（位于 `/Volumes/yexiyue/SwarmDrop/crates/core/src/database/ops.rs`）已经完整提供：`get_transfer_history(db, status?)` / `get_session_detail(db, id)` / `delete_session(db, id)` / `clear_all_history(db)` / `mark_session_failed(db, id, reason)`。桌面端 Tauri 命令层就是把这些 ops 直接包了一层 `#[tauri::command]`。
- 桌面端 `src/stores/transfer-store.ts` 已经验证了「`sessions` 内存活跃 + `dbHistory` 全量 DB 历史 + 事件触发 `removeAndRefresh`」的数据模型在生产可用。

**约束**：

- 不能改 `crates/core` 的公共 API 形状（多个仓库共享，要保持 Tauri / mobile 两端 ops 签名一致）。
- mobile-core 的 uniffi 类型与 `entity` / `swarmdrop_core` 类型是两套不同的 ABI（uniffi 不能直接导出 sea-orm 的 `Model`），必须做转换层。
- iOS / Android 都没有 "在文件管理器中显示" 的等价概念，详情页只能用平台的分享机制承接「我想用这个文件」的诉求。
- 历史中存在的 `transferring` 等中间状态可能是「正在传」也可能是「上次崩了没传完」，启动时必须明确清理。

**相关代码地标**：

- [packages/swarmdrop-core/rust/mobile-core/src/transfer.rs:101-220](packages/swarmdrop-core/rust/mobile-core/src/transfer.rs#L101-L220) —— 现有 `#[uniffi::export] impl MobileCore` 6 个方法
- [packages/swarmdrop-core/rust/mobile-core/src/app.rs:34](packages/swarmdrop-core/rust/mobile-core/src/app.rs#L34) —— `db` 字段持有
- [src/stores/transfer-store.ts](src/stores/transfer-store.ts) —— RN 当前 store（无历史）
- [src/app/transfer/index.tsx](src/app/transfer/index.tsx) —— "传输历史" 页（实际只渲染活跃）
- `/Volumes/yexiyue/SwarmDrop/src/stores/transfer-store.ts` —— 桌面端参照实现
- `/Volumes/yexiyue/SwarmDrop/src/routes/_app/transfer/` —— 桌面端 UI 参照

## Goals / Non-Goals

**Goals:**

- 移动端用户可以看到所有已结束的传输（completed / failed / cancelled / paused），并按状态过滤。
- 用户可以在历史里点开任意一条，看到对端、方向、文件清单、耗时、错误原因；对完成的文件可以直接用系统分享菜单"用其他 App 打开"。
- 用户可以从历史里恢复一笔被中断（paused）的传输，复用共享 crate 已实现的断点续传逻辑。
- 用户可以清空全部历史 / 删除单条历史。
- 应用崩溃 / 被系统杀死后重启时，DB 里残留的"半传"状态会被自动标记为失败，不会污染活跃列表。
- RN 端 store 与桌面端 store 的数据模型在概念上对齐（`sessions` + `dbHistory` + 事件驱动），便于跨端复用思路与未来抽公共逻辑。

**Non-Goals:**

- 跨设备 / 跨账号同步历史（只看本机 SQLite）。
- 分页 / 懒加载 / 虚拟列表 —— 用 `FlatList`（或 `FlashList`，如果项目已用）默认实现，等真出现 1000+ 条历史的性能问题再做。
- 历史记录的导出 / 备份。
- 「重新发送」的 native 化 —— 首版 UI 入口存在，调用现有 `prepareSend` + `sendPrepared` 流程，不引入新 native 命令。
- 后台续传 / push 通知 / iOS background fetch —— 这块的复杂度（iOS 严格限制 + Android Doze）值得独立 change，本期只做前台行为。
- 历史的全文搜索（按文件名 / 对端名）。

## Decisions

### D1. 在 mobile-core 新增专用类型，不直接复用 entity 模型

**决策**：在 `mobile-core/src/transfer.rs` 新增 4 个 uniffi 类型，并写显式 `From<entity::transfer_session::ModelEx>` 转换：

```rust
#[derive(uniffi::Record)]
pub struct MobileTransferHistoryItem {
    pub session_id: String,        // Uuid -> String
    pub direction: String,         // "send" | "receive"
    pub peer_id: String,
    pub peer_name: String,
    pub status: MobileSessionStatus,
    pub files: Vec<MobileTransferHistoryFile>,
    pub total_size: u64,
    pub transferred_bytes: u64,
    pub error_message: Option<String>,
    pub save_path: Option<String>,
    pub started_at: i64,           // ms epoch
    pub finished_at: Option<i64>,
}

#[derive(uniffi::Record)]
pub struct MobileTransferHistoryFile {
    pub file_id: String,
    pub name: String,
    pub relative_path: String,
    pub size: u64,
}

#[derive(uniffi::Enum)]
pub enum MobileSessionStatus {
    Transferring, Paused, Completed, Failed, Cancelled,
}
// 对齐 entity::SessionStatus（DB 层 enum，5 个变种）。
// "pending / waiting_accept" 是 RN 活跃 session 的内存 UI 中间态，
// 不进 DB，也不在这层暴露。

#[derive(uniffi::Record)]
pub struct MobileResumeTransferResult {
    pub session_id: String,
    pub direction: String,
    pub peer_id: String,
    pub peer_name: String,
    pub files: Vec<MobileTransferResumedFile>,  // 复用现有类型
    pub total_size: u64,
    pub transferred_bytes: u64,
}
```

**为什么不直接复用 `swarmdrop_core::database::ops::TransferHistoryItem`**：

- 那是给 Tauri/specta 用的，字段类型是 `Uuid`/`DateTime<Utc>`/`entity::SessionStatus`，uniffi 不能直接桥这些。
- 即便能桥，把 sea-orm enum 暴露给 RN 也会让 native 类型升级时与 RN 端类型严重耦合（任何一个 enum 变种新增都会破坏 ABI）。

**为什么用 `String` 表示 `session_id`**：保持与现有 `MobileTransferOffer`、`MobileTransferProgress` 一致（它们的 `session_id` 都是 String），RN 端就不用做类型分支。

**Alternatives considered**：

- 直接序列化为 JSON 字符串 → 失去类型安全，与现有 record 风格不一致，否决。
- 用一个大 `MobileTransferRecord` 替代既有的 progress/offer/history → 改动面太大、破坏 ABI 兼容，否决。

### D2. `list_transfer_history` 用可选 enum 参数 + 全量返回

**决策**：

```rust
#[uniffi::export(async_runtime = "tokio")]
impl MobileCore {
    pub async fn list_transfer_history(
        &self,
        status_filter: Option<MobileSessionStatus>,
    ) -> FfiResult<Vec<MobileTransferHistoryItem>> {
        let db = self.ensure_db().await?;
        let entity_filter = status_filter.map(Into::into);
        let items = swarmdrop_core::database::ops::get_transfer_history(&db, entity_filter).await?;
        Ok(items.into_iter().map(Into::into).collect())
    }
}
```

**为什么不做分页**：首版工作量最小、native ABI 最稳；移动端用户一般传输次数远低于桌面端；FlatList 在 RN 上对几百条 item 的渲染性能足够。出现性能问题再加 `limit` + `cursor` 是非破坏性变更。

**Alternatives considered**：

- `limit/offset` 分页 → 多写 native + RN 触底加载逻辑，首版收益小。
- 全量拉 + 前端 slice → native 仍然全量装载内存，治标不治本，否决。

### D3. `resume_transfer` 直接对齐桌面端：错误冒上来 UI toast

**决策**：

```rust
pub async fn resume_transfer(
    &self,
    session_id: String,
) -> FfiResult<MobileResumeTransferResult> {
    // 1. 查 DB 拿方向
    // 2. 根据方向分发到 transfer.initiate_resume_as_sender / initiate_resume
    // 3. 转换为 MobileResumeTransferResult
}
```

不在 native 端预先 ping 对端是否在线，由共享 crate 的 `initiate_resume*` 自己负责报错，RN 端 `catch` 到 `FfiError` 后 toast 提示 `t\`对方不在线，无法恢复传输\``。

**为什么这样选**：

- 与桌面端实现完全对齐，行为可预期、可跨端复用文档。
- "先 ping 再调用" 是 TOCTOU 反模式：ping 通过到真正发起恢复之间对端可能就掉线，最后还是要靠真实调用的 error 兜底，那不如直接调用。
- 共享 crate 已经处理了所有失败分支，RN 端只需薄薄一层 UX。

**Alternatives considered**：

- 本期不做 resume → 砍掉一个常用功能，与桌面端体验差距过大，否决。
- 在 mobile-core 加在线检测逻辑 → 复杂度上升 + TOCTOU 漏洞，否决。

### D4. 启动 reconcile：把残留中间状态标记为 failed

**决策**：在 `MobileCore::start_node` 内、`ensure_db()` 之后、`runtime::start_node` 之前，调用：

```rust
async fn reconcile_stale_sessions(db: &DatabaseConnection) -> FfiResult<()> {
    use entity::SessionStatus;
    let stale = entity::TransferSession::find()
        .filter(entity::transfer_session::Column::Status.eq(SessionStatus::Transferring))
        .all(db).await?;
    for s in stale {
        ops::mark_session_failed(db, s.session_id, ERROR_APP_INTERRUPTED).await?;
        tracing::warn!("reconciled stale session {} (was Transferring)", s.session_id);
    }
    Ok(())
}
```

DB 层只有 `Transferring` 这一种"in-flight"状态会被进程死亡留脏。`Paused` 是用户主动暂停的合法终态（用户预期可点 resume 续传），不参与 reconcile。终态 Completed/Failed/Cancelled 也不动。

**为什么是 failed 而不是 paused**：

- P2P 会话上下文（QUIC 连接、协商出的 session key、对端在线状态）在进程死亡时全部丢失。
- "paused" 在桌面端的语义是 "用户主动按下暂停按钮，对端仍在线、断点信息有效"。崩溃残留不满足这个语义，用 paused 会误导用户去点 resume 然后必然失败。
- failed + `error_message="app_interrupted"` 给 UI 提供准确信号，可显示 "上次未完成" 的人性化文案，并提供「重新发送」入口（走全新 session，不走 resume）。

**`app_interrupted` 错误常量**：在 `mobile-core/src/error.rs` 加 `pub const ERROR_APP_INTERRUPTED: &str = "app_interrupted"`，RN 端用 `i18n.t({id: "transfer.error.app_interrupted"})` 做翻译映射。

### D5. RN store 数据模型：双源合一 + 事件驱动

**决策**：重写 `transfer-store.ts`，结构对齐桌面端：

```ts
interface TransferState {
  sessions: Record<string, TransferSession>;      // 活跃（内存）
  dbHistory: MobileTransferHistoryItem[];          // 持久化（从 mobile-core 拉）
  pendingOffers: TransferOfferQueueItem[];         // 入站 offer 队列
  lastError: string | null;
}

// 关键事件映射：
//   TransferProgress → updateProgress（写 sessions[id].progress）
//   TransferAccepted → sessions[id].status = "transferring"
//   TransferRejected → sessions delete + toast
//   TransferComplete → removeAndRefresh：先 loadHistory() 再 delete sessions[id]
//   TransferFailed   → 同上
//   TransferPaused   → 同上 + toast "对方已暂停"
//   TransferResumed  → addSession + loadHistory
```

**为什么先 loadHistory 再 delete sessions**：避免活跃列表瞬间空、历史列表还没更新出现的"传输消失了 1 秒"的视觉空窗。桌面端也是这么做的，验证过。

**为什么不在 store 里持久化 `sessions`**：active session 的真实状态权威源在 native 层（QUIC 连接），RN 重建 store 时直接清空、由事件流重灌就行；如果硬要持久化，重启后会拿到与 native 不一致的"幽灵活跃"。

### D6. 详情页：`expo-sharing` 承接「我想用这个文件」

**决策**：

- 详情页文件列表，每行 `Pressable`：
  - 点击 → `Sharing.isAvailableAsync()` 检查后 `Sharing.shareAsync(file_uri, { dialogTitle: file.name })`
  - 长按 → `Clipboard.setStringAsync(file_uri)` + toast "已复制路径"
- iOS / Android 都用同一份代码，由 `expo-sharing` 内部分发到各自系统 API。

**为什么不用 `IntentLauncher` / `QuickLook`**：

- 跨平台分支多、URI scheme 容易踩坑（content:// vs file:// vs ph://）。
- `expo-sharing` 由 Expo SDK 维护，处理了 SAF / FileProvider / iOS document picker 等所有平台细节。
- 「分享」语义比「打开」更通用：用户可以选择"用 X 打开"也可以选择"发到微信/AirDrop"，体验上界更高。

**降级**：`Sharing.isAvailableAsync()` 返回 false 时（某些受限设备），只提供「复制路径」入口。

**新依赖**：`expo-sharing`（package.json），iOS pod install + Android 自动 link。

### D7. 「重新发送」首版走 RN 侧合成，不加 native API

**决策**：详情页"重新发送"按钮点击后：

```ts
// 1. 从 history item 读出原始 file 路径（保存在 file_uri 字段，由 send 端写入时记下）
// 2. 调用现有 prepareSend(files) → preparedId
// 3. 调用 sendPrepared(preparedId, peerId, peerName, fileIds)
```

**为什么不做新 native 命令**：本质上"重发"就是"用同一批文件向同一个对端再发一次 send"，复用现有 happy path 是最便宜的方案。失败分支（文件被删了、对端配对取消了）共享 crate 已经覆盖。

**限制**：只对 `direction === "send"` 的历史可用；`direction === "receive"` 的历史只能等对端再发。详情页要做按钮可见性判断。

### D8. 新增组件，不复用 `recent-transfer-row` 双职责

**决策**：

- 保留 [src/components/recent-transfer-row.tsx](src/components/recent-transfer-row.tsx) 专做"活跃传输"卡片（输入 `MobileTransferProgress`）。
- 新增 `src/components/history-transfer-row.tsx`，输入 `MobileTransferHistoryItem`，专做历史卡片（状态徽章 + 时间 + 总量 + 错误摘要）。
- 共享样式 token 抽到 `src/components/transfer/shared.ts`（方向图标、状态色、卡片容器、字节格式化）。

**为什么不复用一个组件**：

- 数据形态完全不同：active 是 `MobileTransferProgress`（含 speed/eta），history 是 `MobileTransferHistoryItem`（含 finishedAt/errorMessage）。
- 强行合一需要 union 类型 + 大量条件分支，可读性变差。
- 两个组件 + 一份共享样式，是桌面端 `-transfer-item.tsx` + `-history-item.tsx` + `-shared.tsx` 的同款拆法，跨端思路一致。

## Risks / Trade-offs

- **[mobile-core ABI 不兼容]** 新增 5 个方法 + 4 个类型 + 1 个 enum，旧版 RN bundle 不能跑新版 native，反之亦然。
  → 不发布热更，必须商店重新上架；package.json + Cargo.toml 同时 bump 版本号；CI 上 `pnpm install` 后 `ubrn build` 检查生成的 `swarmdrop_mobile_core.ts` 与 RN 端引用是否对得上。

- **[swarmdrop-core git ref 漂移]** mobile-core 的 Cargo.toml 用 `git = ssh://... branch = "develop"`。如果 reconcile 阶段需要 ops 里某个未发布的函数，必须先在 SwarmDrop 仓库 develop 上 push。
  → 实施前确认 `mark_session_failed(db, id, Some(reason))` 在 develop 已 ready；若需要新 ops 函数（不预期，但可能），先 PR 到 SwarmDrop，bump 引用 commit。

- **[启动 reconcile 误杀真实进行中的传输]** 极端情况：用户多开 / 后台 fetch 模式下 native 已在跑传输，UI 进程又被冷启动。
  → 移动端单实例（Expo bare workflow 下 RN bridge 是单例），native `start_node` 也是幂等（第二次调用会 short-circuit），reconcile 只在 `start_node` 首次成功路径里跑；多开问题不在 RN 物理模型内，不防御。

- **[expo-sharing iOS 限制]** Sharing.shareAsync 对 file:// 之外的 URI 要求 `UTI` 元数据，部分透传的 ContentResolver URI 在 iOS 上可能失败。
  → save_location_uri 由 mobile-core 端构造，目前都是 `file://...`（`Paths.document.uri` 基础上拼接），路径风险低；失败时 catch 后回退到 toast + 复制路径。

- **[Resume 在移动场景的成功率]** P2P 链路在手机切网 / 进后台时极易断；resume 调用 native 后真实成功率可能不如桌面端。
  → 失败也只是 toast，不破坏数据；首版接受这个体验落差，后续可考虑「断点续传 + 自动重试」单独立项。

- **[Sea-orm 大查询性能]** 全量 `get_transfer_history(db, None)` 在历史很多时阻塞主线程？
  → 走 uniffi `async_runtime = "tokio"`，Rust 端跑在 tokio runtime；RN 端是 Promise 调用，不阻塞 JS 主线程；DB 是本地 SQLite，1k 条记录预期 ms 级。

- **[`recent-transfer-row` 既有引用]** 当前组件被外部用作"传输历史"卡片，重写 store 后该组件的语义会从"过去 + 现在混合"收窄为"只显示活跃"。
  → 全局 grep 找出所有引用点（应只有 `app/transfer/index.tsx` 一处），明确收窄后语义；命名上保留但语义清晰化。
