# transfer-store-port-completion 设计

闭合两条根因：**R1 端口覆盖不全**（14/20 方法，历史管理六类全在 trait 外）与
**R2 端口无出口**（`TransferManager` 的 store 字段 `pub(crate)` 且无 accessor）。
解锁 issue #104（Web 传输历史只增不减）。

依赖关系：本 change 是 C3 `inbox-store-port-completion` 的前置（`SqlSessionStore` 的
形态先在这里定下来，收件箱再照同一个模子做）。与 C1 / C4 / C5 / C6 无耦合。

---

## D1：补 trait 方法与开 `store()` accessor 必须同一个 change

**选项 A**：先补 trait 方法（PR1），再开 accessor 切调用点（PR2）。
**选项 B**：一次做完。

**结论：B。**

理由不是「省事」，是 A **中间态没有任何价值且会主动误导**。补完 6 个方法之后，
`crates/transfer/src/manager.rs:163` 的 `store` 仍然是 `pub(crate)`，宿主拿不到它，
于是桌面 `commands/transfer.rs:196` 那条 `State<'_, sea_orm::DatabaseConnection>` 一行不动、
MCP `tools.rs:456` 一行不动、移动 `history.rs:237` 一行不动。PR1 合进去的结果是
**trait 上多了 6 个只有测试在调的方法**，端口覆盖率账面从 48% 涨到 100%，实际直连 SeaORM
的调用点一个没少。下一个人读到「端口已经补齐了」，会得到完全错误的结论。

代价是这个 change 比较大（三端 + 两个 crate）。用任务分阶段（tasks.md 的 Phase 1→6）
控制，每阶段结束都能 `cargo check --workspace` 通过。

---

## D2：端口方法签名一律纯 DTO，`ModelEx` 不许上 trait

`entity::transfer_file::ModelEx` / `transfer_session::ModelEx` 是 `#[sea_orm::model]`
生成的**带关系字段**的类型（`HasOne` / `HasMany`）。它们出现在 `crates/transfer` 的公开签名上，
等于把 sea-orm 的关系机制拖进一个必须编 wasm 的 crate。

所以 6 个新方法的返回值全部是已有的纯 DTO：

| 方法 | 返回 | DTO 定义处 |
|---|---|---|
| `list_transfer_projections` | `Vec<TransferProjection>` | `transfer/src/store.rs:158`（纯 scalar + `Vec<TransferProjectionFile>`） |
| `get_session_source_paths` | `Vec<String>` | — |
| `reap_expired_suspended_receives` | `Vec<ExpiredReceiverActor>` | `transfer/src/store.rs:148`（`Uuid` + `Vec<HostFileMetadata>`） |
| `delete_session` / `clear_all_history` | `()` | — |
| `update_session_origin` | `()`，入参 `TransferOrigin` | `transfer/src/protocol`，纯 enum |

**连带的一处清账**：`content_root_of`（`transfer/src/store.rs:212`）现在吃
`impl IntoIterator<Item = &ModelEx>` + `Option<&entity::SaveLocation>`。正因为它吃 `ModelEx`，
Web 侧编不过，只好在 `web/src/store.rs:665` 抄了一份同语义的（该处注释白纸黑字写着
「避免为 `content_root_of` 的 `ModelEx` 签名引入 sea-orm」）。

签名改成 `(files: &[entity::transfer_file::Model], save_path: Option<&CoreSaveLocation>)`
之后，Web 那份直接删掉改调共享的。两个调用点跟着调整：

- `transfer/src/store.rs:231`（`From<ModelEx> for TransferProjection`）——
  先 `Vec<Model>` 化再传，或就地按 scalar 字段构造
- `storage-sql/src/inbox.rs:188`（收件箱建条目时算 `root_path`）——同款

`Model` 是纯 scalar 结构（`web/src/store.rs` 模块注释第 7 行已实测确认「可直接手构造」），
上签名安全。**双份实现合一是签名修正的自然结果，不是额外目标。**

---

## D3：投影排序写进端口契约（正面推翻 Web 现有注释）

现状分叉：桌面 `ops.rs:456` 有 `order_by_desc(StartedAt)`；Web `web/src/store.rs:129`
的 `all_projections()` 直接 `HashMap::values()` 出，**顺序完全不确定**（HashMap 迭代序）。
:127-128 有一段注释为此辩护：

> 不排序：两个消费面板（收件箱按结束时间、活动视图按更新时间）排法本就不同，
> 且投影经 JS 对象汇入 store 后顺序不再可依赖——排序职责单点留在前端。

**这条理由半对半错，结论错。**

对的部分：前端确实要按自己的维度再排一次。桌面 MCP 的 `list_transfers`
（`tools.rs:461`）拿到有序结果后还是 `sort_by_key(Reverse(updated_at))` 重排了一遍。

错的部分：「前端还要再排」不能推出「端口可以返回任意顺序」。端口一旦允许不确定顺序：

1. 同一份数据两次调用可以给出不同顺序 —— 测试没法断言，只能断言集合相等，
   于是「桌面按 started_at 倒序」这条现有行为**没有任何测试保护**
2. 任何「取最近 N 条」的消费方（Web `transfer-activity-panel.tsx` 的 `HISTORY_LIMIT = 8`
   就是一个）在 Web 上取到的是随机 8 条
3. trait 的意义就是让三端行为可互换。一个「顺序未定义」的方法是伪统一

**结论：契约定为「按 `started_at` 倒序」**，Web 实现补
`sort_unstable_by_key(|p| Reverse(p.started_at))`。前端各面板照旧按自己的维度重排 ——
端口给的是**确定性**，不是最终展示序。Web 那段注释改写成指向本决策。

---

## D4：「进行中不可删」升级为域层不变量，守卫放在 `TransferManager`

现状：桌面删除按钮只在 `!isActive` 时渲染（`src/routes/_app/transfer/-session-row.tsx:298`，
`isActive` = `offered | waiting_accept | active`，见 `src/lib/transfer-projection.ts:4`）。
但 `delete_transfer_session` 命令与 `ops::delete_session:461` **都没有守卫** ——
MCP 客户端、陈旧的前端状态、或即将新增的 Web 入口都能删掉一个正在传的会话。
那会留下一个还在跑、还在往已删除的行写 checkpoint 的 actor。

三个放置守卫的选项：

| 选项 | 位置 | 问题 |
|---|---|---|
| A | 各宿主 UI（现状） | 三端各写一次，MCP 与 wasm 导出都绕得过 |
| B | 两个端口实现里各判一次 | 逻辑重复两份，SQL 与 Web 有分叉机会；且端口是存储层，不该持有生命周期策略 |
| C | `TransferManager::delete_session()` | ✅ |

**结论：C。** 新增

```rust
impl TransferManager {
    /// 删除一条传输记录。非终态一律拒绝——「进行中」应先 cancel（C1 已给两端补齐取消）。
    pub async fn delete_session(&self, session_id: Uuid) -> AppResult<()>;
}
```

它先 `self.store().find_session(id)`，用 `transfer::store` 里新增的共享判据
`pub fn is_deletable(session: &Model) -> bool`（= `phase == Terminal || phase == Suspended`）
判定，再委托 `store().delete_session()`。判据是**一个自由函数、一处定义**，
不因为守卫在域层就把 `phase` 语义抄第二遍。

副作用正是本 change 想要的：宿主的删除路径从「拿 DB 连接自己 delete」变成
「调 manager 的域方法」，`store()` accessor 只服务**纯读**（`list_transfer_projections` /
`get_session_source_paths`）与**无生命周期语义的写**（`update_session_origin`）。
两类调用形态由此清晰分开。

`suspended` 允许删（它是终态的近亲：没有活 actor），代价是断点信息一并消失 ——
桌面确认卡文案已经这么写了（`-session-row.tsx:323`：「删除后该任务的断点信息将一并清除，
无法再继续续传」），本 change 只是让后端也真的这么保证。

---

## D5：删记录 ≠ 删文件；与收件箱的分工（issue #104 的两条「待定」）

issue #104 把这两条列为待定。查代码发现**桌面早已定死答案，只是没写成规格**：

1. **不删收件箱条目。** `crates/migration/src/m20260627_000002_drop_inbox.rs:28-30` 把
   `inbox_items.transfer_session_id` 建成 `ON DELETE SET NULL`；`crates/entity/src/inbox_item.rs:12`
   的字段注释直说「活动账本被清理后这里会置空，收件箱内容仍保留」。
2. **不删已落盘文件。** `src/routes/_app/transfer/-session-row.tsx:325` 的确认文案
   「已传输的文件不受影响」。

**结论：Web 照抄同一语义**，不因为「Web 的文件在 OPFS 里」就分叉。三条支撑：

- **一致性**：一个「删传输记录」在桌面不删文件、在 Web 删文件，是最糟的那种平台分叉 ——
  用户不会读两套文档。
- **收件箱是文件的所有者。** `docs/app/app/inbox/page.tsx:8-10` 的分工注释已经定了：
  「收件箱是**结果** —— 已落盘、可下载、可长期回看；传输页是**过程**」。
  文件生命周期属于结果那一侧。删传输记录时连带删 OPFS 文件，等于让「过程」页删掉「结果」页
  还在展示、还能下载的东西。
- **现在也做不到。** `crates/web/src/opfs.rs` 全部能力只有
  `opfs_file_handle` / `open_writable` / `export_blob_url`，**没有 `removeEntry`**；
  `OpfsFileAccess::cleanup_sink`（`web/src/file_access.rs:155`）只是
  「移除即 drop writable 句柄」，不碰 OPFS 里的文件。真要删文件得先补 OPFS 删除能力。

**前瞻定义（给 C3）**：Web 建真收件箱表后，「释放 OPFS 空间」的入口在**收件箱**
（删收件箱条目 → 连带删 OPFS 文件），传输页的删除永远只删记录。C3 落地时把
`opfs::remove_entry` 补上，并给收件箱删除加二次确认。本 change 在
`crates/web/src/store.rs` 与传输页删除确认文案里写明「文件不受影响，如需删除请到收件箱」。

**已知负债（本 change 不修，明确记账）**：删掉一个 `suspended` 的**接收**会话后，
其 `.part` 会成为孤儿（DB 行没了，`reap` 再也扫不到它）。桌面上它躺在用户的接收目录里，
用户可见可删；Web 上它占 OPFS 配额且用户完全看不见。修它需要 OPFS 删除能力，
随 C3 一起做。

---

## D6：`clear_all_history` 契约收窄为「只删终态会话」

现状 `ops.rs:473` 是两条无条件 `delete_many()`：

```rust
entity::TransferFile::delete_many().exec(db).await?;
entity::TransferSession::delete_many().exec(db).await?;
```

用户在传输页点「清空活动记录」时如果正好有一条在传，那条的行会被删掉，而 actor 还活着，
之后 `apply_transition` 会对着一个不存在的 session 写 —— 与 D4 拒绝删除单条进行中会话
自相矛盾。

**结论：契约收窄为「删除所有终态（`phase = Terminal`）会话及其文件行；非终态保留」。**
名字保留 `clear_all_history` —— 「history」本来就指已经成为历史的那部分，进行中的会话
还不是历史。SQL 实现加 `.filter(Column::Phase.eq(TransferPhase::Terminal))`（文件行按
子查询或先查 id 再删），Web 实现同款过滤。

这是**行为变更**，但共享契约第 4 条已明确「不考虑向后兼容性，该重构的重构」。
桌面前端清空后的 `loadProjections()` 会把幸存的进行中会话刷回来，UI 不需要改。

---

## D7：`reap` 进 trait 后，Web 的调用时机从 `load()` 挪到 `spawn()`

`reap_expired_suspended_receives` 在 Web 侧「已有等价实现，是搬家不是新写」这个说法
**只对了一半**。Web 的实现（`web/src/store.rs:229` `is_expired_recoverable_receive` +
:243 `reap`）跑在 `PersistentSessionStore::load()`（:106）里 —— 即**节点起来之前**。
因此它的命中判据与桌面不同，:226-228 的注释解释了为什么：

> Web 的回收发生在加载期（清理还没跑），遗留的 `Active` 尚未转 `Suspended`，
> 故这里按「非终态」判而不按 `phase=Suspended` 判

**结论：把 Web 的 reap 调用挪到 `WebNode::spawn` 里 `cleanup_recoverable_sessions()` 之后**
（`crates/web/src/node.rs:214-230` 那一段），与桌面 `cleanup_stale_sessions`
（`src-tauri/src/database.rs:47`，先 cleanup 后 reap）和移动
`reconcile_stale_sessions`（`mobile-core/src/history.rs:205`，同序）完全对齐。

收益不是形式统一 —— 是那条**分叉的判据直接消失**：调用点对齐后，Web 也只需判
`phase == Suspended`，可以和桌面共用同一段文字契约，`store.rs:223-237` 那两个自由函数
连同它们的解释性注释一起删掉。

**Web 仍不清理 `.part`，这是刻意的。** `swarmdrop_transfer::cleanup_expired_part_files`
（`transfer/src/lib.rs:70`）的做法是 `open_or_create_sink` → `cleanup_sink`。在 Web 上：
`OpfsFileAccess::open_or_create_sink` 会**创建**缺失的文件，`cleanup_sink`
（`file_access.rs:155`）只 drop 句柄不删文件 —— 调它的净效果是**凭空造出一批空文件**。
所以 Web 侧只做 DB 侧回收，`.part` 清理与 D5 的孤儿文件问题一起归 C3。
（桌面走的是 `database.rs:61-76` 的直接 `tokio::fs::remove_file`，不经 `FileAccess`；
移动走 `cleanup_expired_part_files`。这处三端本来就不一致，本 change 不动它。）

---

## D8：删掉 `mark_session_completed` / `mark_session_paused`

`ops.rs:278` / `ops.rs:312` 是生命周期重构（`redesign-transfer-lifecycle`）的遗物 ——
它们直接写 `phase` / `status` / `terminal_reason`，**绕过 `apply_transition` 这个
Coordinator 唯一的状态持久化入口**。`ops.rs:287-288` 自己的注释就写着
「后续 Coordinator 接线后状态决策收归 `dispatch`，这些 `mark_*` 将被替换」。

生产调用点：**零**。现存调用全部在测试里，且**跨 crate**（审计原稿只提到 storage-sql 内部，
实际还有一处在 src-tauri）：

- `crates/storage-sql/src/ops.rs:648` / :650（`ops` 自己的测试）
- `crates/storage-sql/src/inbox.rs:606` / :718 / :748 / :769 / :801 / :847
- `crates/core/tests/e2e_transfer.rs:311`
- **`src-tauri/src/database.rs:207`**（`cleanup_expired_receiver_suspended_removes_part_file_and_fails_session`）

**结论：删。** 测试改用两条正路之一：需要真实状态机语义的走
`TransferCoordinator::dispatch` / `apply_transition`；只需要一行「处于某 phase」的 fixture 的，
直接构造 `entity::transfer_session::ActiveModel` 落库。留着它们的代价是每个读代码的人
都要判断一次「这条路和 coordinator 那条哪个是真的」。

`update_sender_file_progress`（`ops.rs:203`）不删，降 `pub` → private：唯一调用者是同文件
:254 的 `save_sender_file_progress`，它本身是端口方法。

---

## D9：`store()` 返回 `&Arc<dyn TransferStore>`

与紧邻的 `file_access()`（`manager.rs:261`，返回 `&Arc<dyn FileAccess>`）保持同形。
调用方要跨 await 持有时自己 `.clone()`；返回 `&Arc` 让「只调一个方法就走」的绝大多数
调用点免掉一次 refcount 往返。放在 `endpoint()` / `file_access()` 同一个 impl 块里，
文档注释写明「宿主经此取回自己注入的 store 做历史查询；带生命周期语义的写走
`delete_session()` 这类域方法（D4）」。

---

## D10：Web 的 `all_projections` 同步 → async 的连锁改动

`SessionStore` 是 `#[async_trait]`，所以 `list_transfer_projections` 必然是 `async fn`。
而 Web 现在的 `all_projections()`（`web/src/store.rs:129`）是**同步** fn ——
它只读内存 `HashMap`，不碰 IndexedDB。变 async 是纯签名成本，无运行时代价
（wasm 单线程，poll 一次即完成），但连锁三处：

1. `crates/web/src/node.rs:639` `transfer_history()` → `pub async fn`
2. `docs/packages/swarmdrop-web/swarmdrop_web.d.ts` 里 `transfer_history(): TransferProjection[]`
   → `Promise<TransferProjection[]>`（`pnpm build:wasm` 重新生成）
3. `docs/app/app/_components/web-node-bootstrap.tsx:68`
   `webNodeActions.setHistory(node.transfer_history())` → await

第 3 处有个时序要点必须保住：那一段的注释写着「源三先于源一：历史回补是同步快照，
先灌进去就不会与随后的实时事件抢同一个 sessionId」。改 await 后
`startEventConsumption(node)` 必须仍在 await **之后**调用，否则事件流可能先于历史回补跑起来，
而 `setHistory` 的「已存在的不覆盖」策略（`_lib/store.ts:163-169`）正是靠这个顺序才对。
tasks 里单列一条并加注释说明。

---

## D11：桌面 Tauri State 里的 `DatabaseConnection` 收缩到什么程度

本 change 后，`app.manage(db)`（`src-tauri/src/setup.rs:250`）仍然保留，因为：

- **收件箱命令**（`commands/inbox.rs` 全文 12 处）与 MCP 的收件箱工具
  （`mcp/tools.rs:383` / :417 / :586 / :618 / :771 / :793）还在直连 —— 那是 C3 的范围
- `init_database` / `cleanup_stale_sessions` 需要连接来**构造** `SqlSessionStore`

本 change 只保证：**传输相关**的读写一条不剩地走端口。C3 做完后 `DatabaseConnection`
就只剩「构造两个 store」这一个用途，届时可以考虑不再 `manage` 它。

顺带摘掉审计原稿没列的第五条：`commands/transfer.rs:259` 的 `resume_transfer` 也吃
`DatabaseConnection`，但它只用来 `find_by_id` 取 `session.direction`
（:266-269）—— 端口早就有 `find_session`，直接换掉。

---

## D12：移动端只补 `get_transfer_source_paths`，不碰 cancel/pause

移动端有 7 项 uniffi 桥接缺口，本 change 只补其中一项：`get_transfer_source_paths`。
理由是它**与本 change 的端口方法一一对应** —— `get_session_source_paths` 从自由函数变端口
方法的同时，移动端顺手拿到出口，「从历史重新发送」这条桌面有、移动没有的路补齐。

`cancel_transfer` / `pause_transfer` 的方向合并（`mobile-core/src/transfer.rs:244` 靠
「先试 cancel_send 失败再试 cancel_receive」判方向）属于 C1，本 change 不动 ——
它跟持久化端口没有任何关系，混进来只会让这个已经很宽的 change 更难 review。

---

## 三条 wasm 硬约束：本 change 是否触碰

| 约束 | 触碰？ | 说明 |
|---|---|---|
| **`crates/core` 零 sea-orm** | **否** | core 只经 `pub use swarmdrop_transfer as transfer`（`core/src/lib.rs:20`）转口，本 change 不往 core 加任何依赖 |
| **`crates/transfer` 零 network 依赖** | **否**（但表述需修正，见下） | 新增的 6 个方法都在 `store.rs`，只用 `uuid` / `entity::Model` / 自己的 DTO；`TransferManager::delete_session`（D4）只碰 `store` 字段，不碰 `endpoint` |
| **`crates/invite` 零 core 依赖** | **否** | 本 change 完全不碰 `crates/invite` |

**表述修正**：「`crates/transfer` 零 network 依赖」在本仓的准确含义是
**「不依赖 sea-orm，也不依赖 core 的 `network` / `pairing` 模块」**，
而**不是**「不依赖任何网络 crate」—— `crates/transfer/Cargo.toml` 里
`swarmdrop-net = { workspace = true }` 是实打实的依赖（`TransferManager` 持
`endpoint: Endpoint`），该文件顶部注释原文也是「不依赖 sea-orm / pairing / network **模块**」。
后续 change 的 design 沿用这个更精确的说法，免得有人照字面去删 `swarmdrop-net`。

**真正的风险点只有一个：D2 的 DTO 纪律。** 6 个方法里最容易破功的是
`list_transfer_projections` —— SQL 侧的自然写法是返回
`Vec<entity::transfer_session::ModelEx>` 再让调用方 `.into()`。**不许。**
转换必须发生在 `SqlSessionStore` 的方法体内部（就像 `ops.rs:359` 现在做的
`sessions.into_iter().map(Into::into)`），trait 上只见 `TransferProjection`。
tasks 的 Phase 6 把 `./scripts/check-wasm.sh --clippy` 放在每个 Rust 阶段结束后各跑一次，
不留到最后 —— 破了这条纪律的编译错误在 wasm target 上才暴露，越晚发现改动面越大。
