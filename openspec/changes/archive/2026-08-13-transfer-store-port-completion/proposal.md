## Why

传输域早在 `storage-abstraction` 时就把持久化倒置成了端口（`crates/transfer/src/store.rs`
的 `SessionStore`:21 / `InboxStore`:113），但那次只倒置了**运行时写路径**。三端各自需要的
「历史管理」六类操作，一条也没进 trait：

| 能力 | 现居 | trait 里有吗 |
|---|---|---|
| 列全部投影 | `storage-sql/src/ops.rs:355` | 否 |
| 删单条会话 | `ops.rs:461` | 否 |
| 清空历史 | `ops.rs:473` | 否 |
| 取源文件路径（重新发送） | `ops.rs:492` | 否 |
| 过期挂起接收会话回收 | `ops.rs:513` | 否 |
| 更新会话 origin（MCP 代收标记） | `ops.rs:128` | 否 |

**端口只覆盖了一半，另一半靠自由函数私下走 SeaORM。** 后果不是审美问题，是三端行为已经分叉：

- 桌面 `src-tauri/src/commands/transfer.rs` 的 :195 / :203 / :212 / :221 四条命令、
  `resume_transfer`:259，以及 MCP 的 `tools.rs`:456 / :486 / :674，全部直接吃
  `State<'_, sea_orm::DatabaseConnection>`。
- 移动 `mobile-core/src/history.rs` 的 :237 / :249 / :258 / :265 同款直打 `ops`；
  而 `get_session_source_paths` 在 SQL 侧存在、移动端**根本没有 uniffi 出口**，
  于是「从历史重新发送」这条桌面有的路，移动端没有。
- Web 只能自己在 `crates/web/src/store.rs:129` 造一个 `all_projections()`，且**故意不排序**
  （:127-128 有注释说明理由）；删除/清空则整个不存在 —— 这就是 issue #104
  「传输历史只增不减」。

**而且光补 trait 方法解决不了问题。** `crates/transfer/src/manager.rs:163` 的
`store: Arc<dyn TransferStore>` 是 `pub(crate)` 且**没有任何 accessor**（该 impl 块只有
`endpoint()`:257 与 `file_access()`:261）。宿主把 store 注进去之后就再也拿不回来，所以：

- 桌面把 `DatabaseConnection` 另存一份进 Tauri State（`commands/transfer.rs:196`）
- Web 把 store 存成**具体类型** `Arc<PersistentSessionStore>`（`crates/web/src/node.rs:139`），
  绕过 trait 直接调 `all_projections()`

**端口无出口是根因，补方法是治标。** 只加 trait 方法而不开 `store()`，桌面命令照样会继续
拿着 `DatabaseConnection` 打 SeaORM —— 所以这两件事必须在同一个 change 里做完。

顺带，issue #104 列为「待定」的删除语义，其实桌面早已用两处代码定死了答案，只是没写成规格：

- `crates/migration/src/m20260627_000002_drop_inbox.rs:28-30` —— `inbox_items.transfer_session_id`
  是 `ON DELETE SET NULL`，配 `crates/entity/src/inbox_item.rs:12` 的注释
  「活动账本被清理后这里会置空，收件箱内容仍保留」。**删传输记录不删收件箱条目。**
- `src/routes/_app/transfer/-session-row.tsx:325` 的确认文案「已传输的文件不受影响」。
  **删传输记录不删已落盘文件。**

同时暴露两个真实缺口：删除按钮的「进行中不可删」只写在 UI（`-session-row.tsx:298` 的
`!isActive`），后端命令与 `ops::delete_session` 都无守卫，MCP 或陈旧前端可以绕过；
`clear_all_history`（`ops.rs:473`）更是无条件 `delete_many()`，会把正在传的会话行一起删掉，
留下一个还在写 checkpoint 的孤儿 actor。

## What Changes

- **`SessionStore` 14 → 20 方法**（`crates/transfer/src/store.rs`）：补齐
  `list_transfer_projections` / `delete_session` / `clear_all_history` /
  `get_session_source_paths` / `reap_expired_suspended_receives` / `update_session_origin`。
  全部用纯 DTO 签名（`Vec<String>` / `Vec<TransferProjection>` / `Vec<ExpiredReceiverActor>`），
  **绝不出现 `entity::…::ModelEx`** —— 那会把 sea-orm 的关系类型拖回 `crates/transfer`，
  `./scripts/check-wasm.sh` 当场变红。

- **开 `TransferManager::store()` accessor**（`manager.rs`，紧挨 `file_access()`）。
  这是本 change 的支点：没有它，上一条补得再全，宿主也只能继续绕路。

- **`storage-sql` 的自由函数降为 `SqlSessionStore` 的 impl 方法**：上述 6 个从 `ops.rs`
  搬进 `store.rs` 的 impl；`update_sender_file_progress`（`ops.rs:203`，唯一调用者是同文件
  :254）降 private；**删掉 `mark_session_completed`:278 与 `mark_session_paused`:312** ——
  它们绕过 `apply_transition` reducer 直写 phase，是生命周期重构留下的遗物，
  生产代码零调用（现存调用点全在 `ops.rs` / `inbox.rs` / `src-tauri/src/database.rs` 的
  `#[cfg(test)]` 里，一并改写为经 coordinator 或直构 fixture）。

- **`content_root_of` 双份实现合一**（`transfer/src/store.rs:212` 与
  `web/src/store.rs:665`）：签名从 `&ModelEx` 改成 `&[Model]` + `Option<&CoreSaveLocation>`，
  Web 那份删除、改调共享的。这处重复本来就是「签名吃了 ModelEx，wasm 侧只好抄一份」
  逼出来的，签名一改它自然消失。

- **三端切到端口**：
  - 桌面五条命令 + MCP 三处改走 `net.transfer().store()`；`cleanup_stale_sessions`
    （`src-tauri/src/database.rs:47`）改用它已经构造好的 `SqlSessionStore`；Tauri State 里的
    `DatabaseConnection` 收缩到只剩 inbox 与 MCP 的 inbox 查询用（收件箱是 C3 的事）
  - 移动 `history.rs` 四条同款改造，并**新增 `get_transfer_source_paths` uniffi 导出**，
    补上「从历史重新发送」的缺口
  - Web `all_projections` 变成 trait 方法 `list_transfer_projections`（**从同步 fn 变 async**），
    `node.rs:640` 的 `transfer_history()` 与
    `docs/app/app/_components/web-node-bootstrap.tsx:68` 跟着改成 await

- **投影排序统一**：trait 契约规定按 `started_at` 倒序。桌面本来就有
  （`ops.rs:456` 的 `order_by_desc(StartedAt)`），Web 补上。这**正面推翻**了
  `web/src/store.rs:127-128` 那条「排序职责单点留在前端」的注释 —— 理由见 design D3。

- **Web 补删除能力（issue #104）**：`crates/web/src/node.rs` 新增
  `delete_transfer_session` / `clear_transfer_history` 两个 wasm 导出，
  `docs/app/app/_components/transfer-activity-panel.tsx` 加删除入口，清空走二次确认。

- **把「进行中不可删」从 UI 约定升级成域层不变量**：新增
  `TransferManager::delete_session()`，非终态一律拒绝；`clear_all_history` 的端口契约
  收窄为「只删终态会话」。两处都补测试。

**非目标**：

- **收件箱端口**（`InboxStore` 1 → 11、`storage-sql/src/inbox.rs` 降私有、Web 建真收件箱表）
  → C3 `inbox-store-port-completion`。本 change 只定义「删传输记录 ≠ 删收件箱条目」这条分工，
  不动收件箱任何一行代码。
- **OPFS / 磁盘文件的删除**。本 change 明确定义为「删记录不删文件」，与桌面现状一致；
  Web 侧的 OPFS 文件清理归收件箱所有（C3），且当前 `crates/web/src/opfs.rs` 根本没有
  `removeEntry` 能力 —— 见 design D5。
- **取消 / 暂停的两端拆分**（→ C1）、**设备名进配对请求**（→ C5）、
  **原子解除配对**（→ C4）、**agent_version 运行时更新**（→ C6）。
- **不做向后兼容垫片**。trait 方法名与签名按现在该长的样子定，旧自由函数直接消失。

## Capabilities

### New Capabilities

- `transfer-persistence-port`: 传输域的持久化经**唯一一组端口 trait** 表达并**有出口** ——
  运行时写路径与历史管理（列表 / 删除 / 清空 / 源路径 / 过期回收 / origin 标记）全部在
  `SessionStore` 上；宿主经 `TransferManager::store()` 取回自己注入的实现，不再另存 ORM
  连接或具体类型。三端由此拿到同一套语义：投影顺序确定、删记录不删文件与收件箱条目、
  进行中不可删、清空只清终态。

### Modified Capabilities

<!-- 无已归档 spec 被修改：过期回收的现行规格 `stale-receive-session-expiry` 只约束
     「回收什么」与「两端一致」，本 change 改的是它的**承载形式**（自由函数 → 端口方法）
     与 Web 的调用时机，不改变任何一条已声明的回收判定。 -->

## Impact

- **`crates/transfer`**：`store.rs` 的 `SessionStore` +6 方法、`content_root_of` 换签名、
  新增 `is_deletable`；`manager.rs` 新增 `store()` 与 `delete_session()`。
- **`crates/storage-sql`**：`ops.rs` 的 6 个自由函数搬进 `store.rs` 的 impl，
  `update_sender_file_progress` 降 private，`mark_session_completed` / `mark_session_paused`
  删除（连带改 `inbox.rs`、`core/tests/e2e_transfer.rs`、`src-tauri/src/database.rs` 里的测试）。
- **`src-tauri`**：`commands/transfer.rs` 五条命令摘掉 `DatabaseConnection` State；
  `mcp/tools.rs` 三处改走端口；`database.rs` 的启动清理改用它已构造的 `SqlSessionStore`。
  **`src/lib/bindings.ts` 预期零 diff**（State 参数不进 IPC 签名）。
- **`mobile/`**：`mobile-core/src/history.rs` 六处改走端口 + 新增
  `get_transfer_source_paths` uniffi 导出；RN 侧 `transfer-store` 与传输详情页的
  「重新发送」升级为真重发（带路径失效回退）。
- **`crates/web` / `docs/`**：store 实现补 6 方法、reap 调用点从 `load()` 挪到 `spawn()`、
  本地 `content_root_of` 删除；新增两个 wasm 导出；传输页加删除与清空入口。
  `transfer_history()` 变 async 是跨 wasm 边界的破坏性签名变更，`pnpm build:wasm` 必须重跑。
- **回归重点**：桌面「清空活动记录」的行为变了（不再清进行中）；
  Web 的历史列表从「随机顺序」变「started_at 倒序」；
  过期回收在 Web 上的命中时机从加载期挪到启动清理后。

**风险**：

1. **DTO 纪律破功**（design 末节）。`list_transfer_projections` 的 SQL 侧自然写法会想返回
   `ModelEx`，一旦漏进 trait 签名，`crates/transfer` 被拖回 sea-orm，
   `./scripts/check-wasm.sh` 才会红 —— 所以每个 Rust 阶段结束都要跑它，别攒到最后。
2. **Web 的历史回补时序**。`transfer_history()` 变 async 后，
   `web-node-bootstrap.tsx:68` 的 await 必须仍排在 `startEventConsumption` 之前，
   否则「已存在的不覆盖」策略失效，刷新后可能用落库快照盖掉更新的实时投影。
3. **删掉两个 `mark_session_*` 会波及三个 crate 的测试**（`storage-sql` / `core` /
   `src-tauri`），其中 `src-tauri/src/database.rs:207` 是跨 crate 的，容易漏。
