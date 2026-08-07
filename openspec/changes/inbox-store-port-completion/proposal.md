## Why

收件箱是三端里**端口覆盖率最低**的一块。`crates/transfer/src/store.rs:113` 的 `InboxStore`
只有一个方法（`ensure_inbox_item_for_completed_receive_session`），而
`crates/storage-sql/src/inbox.rs` 里有 **10 个 `pub` 自由函数**——
`:151` ensure / `:266` repair / `:302` list / `:334` search / `:406` get_detail /
`:426` get_by_session / `:443` mark_opened / `:453` archive / `:467` delete_record /
`:477` mark_file_missing。除第一个外，其余 9 个**全在端口之外**。覆盖率 1/10。

这不是审美问题，它已经产生了三处具体后果：

**1. 两个原生端把 SQL 实现当 API 用。** 桌面 `src-tauri/src/commands/inbox.rs` 的 10 条命令
全部签名成 `db: State<'_, DatabaseConnection>` 并直调 `crate::database::inbox::*`
（`src-tauri/src/database.rs:7` 只是 `pub use swarmdrop_storage_sql::{inbox, ops};` 的一层
转发）；MCP 侧另有 6 处 `self.app.try_state::<DatabaseConnection>()`
（`src-tauri/src/mcp/tools.rs:379 / 411 / 582 / 612 / 765 / 793`）。
移动端 `mobile/packages/swarmdrop-core/rust/mobile-core/src/inbox.rs:8` 一句
`use swarmdrop_storage_sql::inbox as inbox_ops;` 之后，9 个 uniffi 方法逐个直打自由函数。
**桌面与移动各自维护一份「怎么调 SQL」的知识**，而这份知识本该只属于 `storage-sql`。

后果之一已经可见：`mark_inbox_item_file_missing` 的**归属校验只有移动端做了**
（`mobile-core/src/inbox.rs:320-328` 先取 detail、断言 `file_id` 属于该条目），
桌面 `commands/inbox.rs:204 / 206 / 222` 三处直接按全局自增主键写。同一条安全检查
一个宿主有、另一个没有——这正是端口该收口而没收口的东西。

**2. Web 端的收件箱根本没有数据。** `crates/web/src/store.rs:524-534` 的 `InboxStore` impl
是 no-op，注释写得很诚实：「Web 壳没有独立收件箱表，收件箱就是 `direction=Receive` 且
`terminal=Completed` 的会话投影」。前端确实就是这么渲染的——
`docs/app/app/_components/receive-panel.tsx:119-125` 对 `projections` 做了一次
`filter(direction === "receive" && phase === "terminal" && terminalReason === "completed")`。

于是**归档 / 软删除 / 标记已打开 / 检索 / 文件缺失标记这五类语义在 Web 上不存在**，
而且不是「UI 还没做」，是**没有可存的地方**。更硬的一条：传输历史被
`HISTORY_CAP = 100`（`crates/web/src/store.rs:62`）淘汰时，对应的「已接收内容」也一起消失。
桌面上这两件事是**刻意分开**的（`crates/storage-sql/src/inbox.rs:3` 模块注释：「收件箱是
『已接收内容索引』，与 transfer_sessions / transfer_files 的过程账本分开维护」，
`clear_history_should_keep_inbox_records`（`:797`）这条测试就钉着它）。Web 端把这条不变量丢了。

**3. 对照组证明抽象本身没问题，问题在覆盖率。**
`TransferCoordinator::cleanup_recoverable_sessions` 是全仓**唯一**「同一能力三端共用同一实现」
的地方——桌面 `src-tauri/src/database.rs:51-55`、移动
`mobile-core/src/history.rs:210-216`、Web `crates/web/src/node.rs:218-224`
三处都只是把自己的 store 包进同一个 core 原语，各三行。它恰好是走 trait 的那条。
而紧邻的过期回收**不走 trait**（`crates/storage-sql/src/ops.rs:513` 的
`reap_expired_suspended_receives` vs `crates/web/src/store.rs:229` 的
`is_expired_recoverable_receive` + `:243` 的 `reap`），于是同一条规则写了两份，
并且两份的命中条件已经因为「Web 在加载期回收、还没跑 cleanup」而**不得不分叉**
（`crates/web/src/store.rs:223-228` 的注释就在解释这件事）。

抽象没问题，覆盖率有问题。本 change 把收件箱这一块的覆盖率补到 10/10。

## What Changes

- **`InboxStore` 从 1 个方法补齐到 10 个**（`crates/transfer/src/store.rs:113`）。
  逐条对齐 `crates/storage-sql/src/inbox.rs` 的 10 个自由函数，**方法名与自由函数逐字相同**
  （保 grep 可达）。签名一律走纯 DTO——绝不出现 `entity::inbox_item::ModelEx` /
  `entity::transfer_file::ModelEx` 这类携带 sea-orm 关系类型的东西（`ModelEx` 是
  `HasMany` / `HasOne` 的宿主，吃进来就把 transfer 拖回 sea-orm，`./scripts/check-wasm.sh` 会红）。
  两处签名与现状**有意不同**，见 design D3（`ensure_*` 的返回值）与 D4（`mark_file_missing` 加 `item_id`）。

- **5 个收件箱 DTO 从 `storage-sql` 迁到新的 `crates/transfer/src/inbox.rs`**：
  `InboxItemSummary` / `InboxItemFileEntry` / `InboxItemDetail` / `InboxSearchHit` /
  `InboxHitFile`（现址 `crates/storage-sql/src/inbox.rs:17-90`）。它们本来就是纯数据
  （字段只有标量 + `entity` 的两个普通 enum + 已在 transfer 的 `TransferProjection`），
  放在 SQL 实现里只是历史位置。`From<ModelEx>` 的转换**留在 SQL 侧**——那是实现细节。

- **五条领域规则上提，两端共用一份**（design D2）：条目标题（`inbox_title`）、
  内容指纹（`inbox_content_hash`，blake3）、检索文本聚合、`source_kind` 派生、
  检索片段生成（`make_snippet` / `snippet_window`）。它们现在全是
  `crates/storage-sql/src/inbox.rs` 的私有函数，而 Web 建了真表以后**必须**产出逐字节相同的
  标题与指纹，否则同一批文件在两端会得到不同的 `content_hash`。
  这照抄 `crates/transfer/src/store.rs:212` 的 `content_root_of` 已有的体例：
  领域规则住在 transfer，各存储实现调它。

- **`crates/storage-sql/src/inbox.rs` 的 10 个自由函数降为 private**，收进
  `impl InboxStore for SqlSessionStore`（`crates/storage-sql/src/store.rs:153`）。
  **注意**：仓库里没有 `SqlInboxStore` 这个类型，`InboxStore` 一直是实现在 `SqlSessionStore`
  上的——`TransferStore: SessionStore + InboxStore` 的 blanket impl
  （`crates/transfer/src/store.rs:126-127`）要求**同一个类型**同时实现两个子端口。

- **Web 建真收件箱表**（不是「让 trait 容纳投影实现」）：
  `crates/web/src/idb.rs` 新增 `INBOX_STORE`（`DB_VERSION` 3 → 4，且 `:142` 的 store 创建
  清单要同步——漏了 `onupgradeneeded` 建不出新 store），`crates/web/src/store.rs:524-534`
  的 no-op impl 换成 10 个方法全实的真实现。收件箱因此**脱离 `HISTORY_CAP` 的淘汰**，
  与桌面「清空活动不动收件箱」对齐。

- **存量 IndexedDB 数据直接丢弃：不回填、不迁移、不双写、不留兼容层**（design D7）。
  升 `DB_VERSION` 只为建出新 store；老库里的已完成接收会话不会被补出收件箱条目。
  理由是 Web 端目前没有真实用户，一切以「架构最合理、最简洁」为准——任何「为了保住旧数据
  而增加的复杂度」在本 change 里都被明确拒绝。

- **宿主组装点收敛**（design D5）：`SqlSessionStore::new(...)` 现在在生产代码里被构造 **4 次**
  （`src-tauri/src/database.rs:52`、`src-tauri/src/commands/lifecycle.rs:84`、
  `mobile-core/src/history.rs:211`、`mobile-core/src/network.rs:235`）。改为各端组装点建一次，
  **注入 `TransferManager` 的与宿主自持的是同一个 `Arc`**。这样收件箱读写不被绑到节点生命周期上
  （桌面现在没启动节点也能翻收件箱，这条行为不能破）。

- **三端调用点切换**：桌面 10 条 Tauri 命令 + 6 处 MCP 工具、移动 9 个 uniffi 方法、
  Web 新增 7 个 wasm 导出，全部改成经端口。

- **Web 前端换数据源**：`docs/app/app/_components/receive-panel.tsx` 的 `InboxPanel` 从
  「过滤 projections」改为读真收件箱条目；`docs/app/app/inbox/page.tsx:8-10` 那条
  「收件箱是**结果** / 传输页是**过程**」的分工注释跟着改写——分工本身没变，但**依据**从
  「同一份 projection 的两种过滤」变成「两张各自的表」，注释不改就会继续误导。

**非目标**：

- **传输历史**（`crates/storage-sql/src/ops.rs` 的 list `:355` / delete `:461` / clear `:473` /
  source_paths `:492` / reap `:513` / update_origin `:128` 六类）→ 属于 C2
  `transfer-store-port-completion`，本 change 依赖它先合（同改两个文件，并行必冲突）。
- **`TransferManager::store()` accessor 本身** → C2 定义。本 change 只在「手里只有 manager」
  的路径上消费它，宿主自己那条走组装点注入（design D5），**不强依赖**。
- **移动端 inbox 的 open / show / export uniffi 桥接** → 立项描述说这是三条缺口，
  **实测不成立**：`mobile/src/app/inbox/[itemId].tsx:200-237` 已在 RN 侧用
  `openFileWithSystem` / `shareFileWithSystem`（`@/lib/open-file`）覆盖「打开」与「分享」，
  失败时还会调 `markFileMissing`。移动端没有「导出到指定目录」这个动作——iOS / Android 上
  它就是系统分享面板。详见 design D11。
- **Web 端收件箱的检索 / 归档 / 删除 UI** → 端口与 wasm 导出本 change 给全，
  但 `docs/app/app` 只做「数据源切换」这一件事，交互面留给后续（design D13）。
- **`.part` 过期回收的三端统一**（`ops.rs:513` vs `web/store.rs:229 / :243`）→
  那是 `SessionStore` 侧的债，且需要「Web 在加载期回收」这条时序差异先被消化，
  不塞进收件箱这条线。
- **向后兼容与数据迁移** → 明确不做，见上文与 design D7。

## Capabilities

### New Capabilities

- `inbox-store-port`: 收件箱的**全部**持久化语义（幂等建条目、补建、列表、检索、详情、
  按会话查、标记已打开、归档、软删除、文件缺失标记）经单一端口 trait 表达，三端各自实现，
  调用方（Tauri 命令 / MCP 工具 / uniffi 桥 / wasm 导出）一律只认端口；
  Web 端拥有独立的收件箱存储，其内容不随传输历史淘汰而消失。

### 与既有 spec 的关系

`inbox-search` 的两条要求（「共享 core SHALL 暴露 `search_inbox(query, limit, include_archived)`」、
「桌面端命令 SHALL 复用与其它 Tauri command 相同的托管数据库连接，不另开连接」）**仍然成立**：
`search_inbox` 变成端口方法后依旧是共享 core 暴露的接口，SQL 实现持有的是同一个
`Arc<DatabaseConnection>`，没有第二条连接。故本 change 不产生 `inbox-search` 的 MODIFIED delta。

## Impact

- **`crates/transfer`**：新增 `src/inbox.rs`（5 个 DTO + 5 条领域规则 + 单测），
  `src/lib.rs` 加 `pub mod inbox;`（**排在 `incoming` 之前**——字母序 `inbox` < `incoming`）；
  `src/store.rs` 的 `InboxStore` 1 → 10 个方法；`src/actor/receiver.rs:709`
  的调用点跟随 D3 的返回值变化（仍然只在 `Err` 分支记日志，不消费 `Ok` 值）。
- **`crates/storage-sql`**：`src/inbox.rs` 的 10 个函数降 private + 改调 transfer 的共享规则；
  `src/store.rs:153` 的 `InboxStore` impl 从 1 个方法长到 10 个；内嵌的 13 条测试改成经
  `SqlSessionStore` 调端口。
- **`crates/web`**：`idb.rs` 加 store + 提版本号；新增 `src/inbox.rs`（真收件箱表）；
  `store.rs` 的 no-op impl 换真实现并改名（design D10）；`types.rs` + `lib.rs` re-export
  收件箱 DTO；`node.rs` 新增 7 个导出；`tests/specta_export.rs` 注册新类型并再生
  `crates/web/bindings/bindings.ts`。
- **`src-tauri`**：`setup.rs` 组装端口实例并 `app.manage`；`commands/inbox.rs` 10 条命令换
  State 类型；`mcp/tools.rs` 6 处换取法；`database.rs` 的 re-export 与启动清理跟着收敛。
  **`src/lib/bindings.ts` 形状预期不变**（DTO 只换 crate，TS 名与字段不变），但仍要再生一次确认。
- **`mobile/packages/swarmdrop-core/rust/mobile-core`**：`inbox.rs` 的 `inbox_ops` 导入换成端口，
  9 个方法改成经 store 调用，5 个 `From` impl 换来源 crate（穷尽解构的 drift guard 保留），
  `:320-328` 的归属校验删除（已上提到端口）。**uniffi 签名形状不变**，不需要重跑 `build:ios`。
- **`docs/`**：`_components/receive-panel.tsx` 的 `InboxPanel` 换数据源；
  `inbox/page.tsx` 的分工注释改写；`pnpm build:wasm` 重新生成 `docs/packages/swarmdrop-web/`。
- **回归**：`clear_all_history` 之后收件箱仍在（`crates/storage-sql/src/inbox.rs:797`）；
  Web 刷新后收件箱条目仍在且不随 `HISTORY_CAP` 淘汰；桌面在**节点未启动**时仍能列出 / 打开
  收件箱（现状如此，D5 的选型就是为了不破它）。

**风险**：

1. **`content_hash` 漂移**。指纹规则一旦在两端不一致，同一批文件会得到不同的
   `content_hash`，而它是未来「跨端去重」的唯一判据。所以规则必须**只有一份**（D2），
   并由 transfer 侧的单测以已知向量钉死。
2. **IndexedDB 版本升级的三处同改**。`DB_VERSION` 3 → 4 会触发所有已打开标签页的
   `versionchange`（`idb.rs:117-125` 已处理让路），但**漏改 `:142` 的 store 创建清单**会让
   新 store 建不出来，且只在运行时报错。`idb.rs:27-29` 的注释里已经警告过一次。
3. **桌面「不启动节点也能看收件箱」的回归**。若组装点做成「唯一持有者是 `TransferManager`、
   宿主经 `store()` 取回」，这条现存行为会静默变成「先启动节点」。D5 就是为此选的形态，
   实现时必须实测这条（tasks 10.13）。
4. **改动面横跨 6 个 crate/包**。端口签名一变，桌面 + 移动 + Web 三端调用点同时红。
   这是不可避免的（端口就是要被三端共用），但意味着本 change 无法拆小合并——
   必须一次改完再过全部门禁。
