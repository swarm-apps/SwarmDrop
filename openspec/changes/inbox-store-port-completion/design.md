# inbox-store-port-completion 设计

依赖 C2 `transfer-store-port-completion`（同改 `crates/transfer/src/store.rs` 与
`crates/storage-sql/src/store.rs`，串行合并；本 change 的「自由函数降 impl」照抄它定的体例）。

## 核实中发现的、与立项描述不符的四处

立项描述来自主线程的代码审计。落到代码上有四处偏差，本设计**以代码为准**：

| 立项描述 | 代码实际 | 影响 |
|---|---|---|
| 「`InboxStore` 1 → 11」，「`:266`…`:477` 共 10 个 pub 自由函数 + 现有的 `ensure_*`」 | 那 9 个行号对应 **9** 个函数（`:266 / :302 / :334 / :406 / :426 / :443 / :453 / :467 / :477`），加上 `:151` 的 `ensure_*` 共 **10** 个 pub 自由函数 | 端口是 1 → **10**，不是 11 |
| 「10 个自由函数降 private，收进 `SqlInboxStore` impl」 | 仓库里**没有 `SqlInboxStore`**。`InboxStore` 一直实现在 `SqlSessionStore` 上（`crates/storage-sql/src/store.rs:153`），因为 `TransferStore: SessionStore + InboxStore` 的 blanket impl（`crates/transfer/src/store.rs:126-127`）要求**同一个类型**同时实现两个子端口 | 不新建类型，方法收进现有 impl |
| 「移动端顺带补 inbox 的 open/show/export 三条 uniffi 缺口」 | `mobile/src/app/inbox/[itemId].tsx:200-237` 已用 `openFileWithSystem` / `shareFileWithSystem`（`@/lib/open-file`）覆盖「打开」与「分享」，失败时还会 `markFileMissing` | 不是缺口，见 D11；已写进非目标 |
| 「Web 端 `store.rs:524-533` 的 InboxStore impl 是 no-op」 | impl 块实际是 `:524-534`（`:526-533` 是那个方法），语义描述无误 | 仅行号，无实质影响 |

其余 file:line 全部核实无误，包括：`crates/web/src/store.rs:62` 的 `HISTORY_CAP = 100`、
`crates/web/src/idb.rs:29 / :32 / :34 / :36 / :142` 的版本号、三个 store 常量与创建清单、
`crates/web/src/node.rs:139` 的 `session_store: Arc<PersistentSessionStore>`、
三处 `cleanup_recoverable_sessions` 共用点（`src-tauri/src/database.rs:51-55` /
`mobile-core/src/history.rs:210-216` / `crates/web/src/node.rs:218-224`）、
以及 4 处 `SqlSessionStore::new` 生产构造点。

另外补一条实现细节的更正：`pub mod inbox;` 在 `crates/transfer/src/lib.rs` 里应插在
**`pub mod flow;`(:14) 与 `pub mod incoming;`(:15) 之间**——按字母序 `inbox` 排在
`incoming` 前面（`b` < `c`），那份模块清单目前是严格字母序的。

---

## D1：DTO 放新模块 `crates/transfer/src/inbox.rs`，trait 留在 `store.rs`

5 个收件箱 DTO 必须从 `storage-sql` 搬进 `transfer`——不搬，端口签名就没法引用它们。
落点有两个选项：

- **(a) 全塞进 `store.rs`**：和 `TransferProjection` 作伴，位置一致。
  代价：`store.rs` 现在 285 行（14 个端口方法 + 5 个类型 + 4 个共享函数），再加 5 个 DTO、
  5 条领域规则与它们的单测会奔着 500 行去，端口定义被数据类型埋掉。
- **(b) 新建 `crates/transfer/src/inbox.rs` 放 DTO + 领域规则，`InboxStore` trait 留在 `store.rs`。**

**选 (b)。** `store.rs` 的模块头第一句就是「持久化端口（依赖倒置的核心）」——它是**端口定义**
文件，`TransferStore` 的 supertrait 组合也在那里，把 `InboxStore` 搬走会让
`TransferStore: SessionStore + InboxStore` 的定义处读起来断成两截。
而 DTO 与领域规则是**数据与规则**，独立成模块后 `crates/web` 与 `crates/storage-sql` 的 `use`
路径也更能说明意图（`swarmdrop_transfer::inbox::{...}`）。

`store.rs` 顶部加 `use crate::inbox::{InboxItemDetail, ...};` 即可，**不做二次 re-export**
——两条路径指向同一类型是新的漂移源。

## D2：五条领域规则上提，两端共用一份

`crates/storage-sql/src/inbox.rs` 里有 5 处编码的是**领域规则**而非 SQL 细节：

| 现址 | 规则 | 为什么必须共用 |
|---|---|---|
| `:501 inbox_title` | 0 个文件 →「空传输」、1 个 → 文件名、多个 →「X 等 N 个文件」 | Web 建真表后要产出**同样**的标题；分叉了同一批文件在两端显示不同 |
| `:509 inbox_content_hash` | blake3(relative_path ‖ 0x00 ‖ checksum ‖ size_le) 逐文件累加 | 这是**跨端去重的唯一判据**，字节级不同就等于这个字段作废 |
| `:522 source_kind_for_origin` | `TransferOrigin::from_db_string` → `Mcp` / `PairedDevice` | `TransferOrigin` 本来就在 transfer（`crate::protocol`），规则跟着它走 |
| `:564 make_snippet` + `:578 snippet_window` | 按字符切 ±16 窗口、UTF-8 安全、首尾加省略号 | Web 没有 FTS，检索片段只能自己生成；两端片段不一致 = 同一次搜索在两端观感不同 |
| `:195-201`（inline） | FTS 聚合文本 `"{name} {relative_path}"` 空格拼接 | 它就是「检索覆盖面」的定义；Web 的子串扫描必须扫**同一段文本**，否则命中集合不同 |

全部上提到 `crates/transfer/src/inbox.rs`，签名用一个中立视图结构，**绝不吃 `ModelEx`**：

```rust
/// 收件箱规则的中立文件视图——两端各自从自己的行类型构造。
pub struct InboxFileFacts<'a> {
    pub name: &'a str,
    pub relative_path: &'a str,
    pub checksum: &'a str,
    pub size: i64,
}

pub fn inbox_title(files: &[InboxFileFacts<'_>]) -> String;
pub fn inbox_content_hash(files: &[InboxFileFacts<'_>]) -> String;
pub fn inbox_files_text(files: &[InboxFileFacts<'_>]) -> String;
pub fn inbox_source_kind(origin: Option<&str>) -> entity::InboxSourceKind;
pub fn inbox_snippet(query: &str, title: &str, source_name: &str, files: &[InboxHitFile]) -> String;
/// 检索命中判据的**规范定义**：大小写不敏感子串，覆盖 title / source_name / files_text。
pub fn inbox_matches(query: &str, title: &str, source_name: &str, files_text: &str) -> bool;
```

`inbox_matches` 只有 Web 会真的调用——SQL 侧那条 `LIKE ... ESCAPE '\'` 是在数据库里做的。
仍然把它放在这里，是因为它是**这条能力的规范定义**：SQL 的那段 raw SQL 在文档注释里指向它，
于是「LIKE 查询必须复刻 `inbox_matches` 的语义」有了一个可读、可测的锚点，
而不是散在两处的口头约定。

`escape_like`（`:530`）**不上提**——那是 SQL `LIKE` 通配符的转义，纯实现细节。

**新依赖检查**：`inbox_content_hash` 要 blake3。`crates/transfer/Cargo.toml` 已有
`blake3 = "1.8.3"`（bao-tree 逐块验签用），**零新增依赖**。

## D3：`ensure_inbox_item_for_completed_receive_session` 的返回值 `()` → `Option<InboxItemDetail>`

现状：端口返回 `AppResult<()>`，SQL 实现用 `.map(|_| ())`（`crates/storage-sql/src/store.rs:161`）
把自由函数的 `Option<InboxItemDetail>` 丢掉；trait 注释解释「调用方 receiver 不消费 detail」。

选项：

- **(a) 保持 `()`。** 好处：接收热路径少一次 DTO 构造。
  坏处：端口比底下的能力**弱**，于是 `repair_missing_inbox_items_for_completed_receives`
  （要返回 `Vec<InboxItemDetail>`）没法用端口的 `ensure_*` 实现，只能在每个存储实现里
  各写一遍「查重 + 构造条目」的循环。
- **(b) 返回 `Option<InboxItemDetail>`**（`None` = 该会话不符合「已完成接收」，**不是错误**）。

**选 (b)。** 决定性理由不是「信息更全」，是**`repair` 能用 `ensure` 实现**：
SQL 侧现在就是这么写的（`crates/storage-sql/src/inbox.rs:286` 在循环里调 `ensure_*`），
Web 侧照抄同一条路。端口若返回 `()`，两个实现里的 repair 就各自重新拼一遍条目构造，
而「条目怎么构造」恰恰是 D2 要收敛的东西。

代价（一次 DTO 构造，含文件列表 clone）落在接收会话**完成**这一次，不在 chunk 热路径上，
可忽略。`crates/transfer/src/actor/receiver.rs:709` 的 `ensure_inbox_item_after_completion`
不改语义：仍然只在 `Err` 分支记日志 + 发 `TransferDbError`，`Ok(_)` 不消费。

## D4：`mark_inbox_item_file_missing` 加 `item_id` 参数

现状 SQL 自由函数是 `(db, file_id: i32, missing: bool)`——`file_id` 是 `inbox_item_files`
表的自增主键、全局唯一，所以够用。桌面命令（`src-tauri/src/commands/inbox.rs:153 / 204 /
206 / 222`）就是这么直调的。但有两条理由让端口签名必须更强：

1. **移动端已经在桥接层补了归属校验**（`mobile-core/src/inbox.rs:320-328`：先取 detail、
   再断言 `detail.files` 里有这个 `file_id`，否则报 `inbox file does not belong to item`）。
   同一条安全检查一个宿主有、另一个没有——这正是端口该收口的东西。
2. **Web 没有自增主键。** IndexedDB 的收件箱文件行只能用「条目内序号」当 id（D9），
   全局不唯一，`mark_inbox_item_file_missing(file_id)` 在 Web 上**无法定位**。

所以端口签名定为 `(item_id: Uuid, file_id: i32, missing: bool)`：
SQL 实现把移动端那段归属校验搬进来（一次按 id 取条目的查询），
Web 实现用 `item_id` 定位条目、`file_id` 定位条目内的文件。
桌面命令与 MCP 侧的调用点补传 `item_id`——那四处都已经在手里有 `detail` 或 `item_id`，**零额外查询**。

## D5：宿主怎么拿到 `InboxStore`——组装点建一次、注入与自持是同一个 `Arc`

这是本 change 里最容易做错的一步。两个选项：

- **(a) 唯一持有者是 `TransferManager`，宿主经 C2 新加的 `store()` 取回。**
  纯度最高：全仓只有一个 store 实例的持有点。
  **但它会引入一个真实的行为回归**：桌面的收件箱命令现在**不依赖节点启动**
  （`src-tauri/src/setup.rs` 早早 `app.manage(db)`，而 `NetManagerState` 是
  `Mutex<Option<..>>`、`with_manager!` 在未启动时返回 `node_not_started`）。
  改走 `manager.store()` 之后，「没联网也能翻已经收到的东西」会变成「先启动节点」。
  收件箱按定义就是**与网络无关的内容账本**（`crates/storage-sql/src/inbox.rs:3` 的模块注释），
  把它绑到节点生命周期上是方向性错误。

- **(b) 组装点建一次 `Arc<dyn TransferStore>`，同一个 `Arc` 既注入 `TransferManager` 的工厂闭包、
  也 `app.manage(...)`（移动端则由 `MobileCore` 持有）给宿主自己用。**

**选 (b)。** 它不是 R2 描述的那个病：R2 的病是宿主**另存了一条通往同一份数据的、绕过端口的路**
（桌面存 `DatabaseConnection`、Web 存具体类型 `Arc<PersistentSessionStore>`）。
(b) 存的是**端口本身**，且是**同一个实例**——没有第二事实源，也没有绕过抽象。
这就是组合根（composition root）的标准形态：宿主创建端口实现，注入需要它的对象，
自己也保留一份用于自己的用例。

顺带的收益是可度量的：`SqlSessionStore::new(...)` 现在在生产代码里被构造 **4 次**
（`src-tauri/src/database.rs:52`、`src-tauri/src/commands/lifecycle.rs:84`、
`mobile-core/src/history.rs:211`、`mobile-core/src/network.rs:235`），每次都新包一层。
(b) 之后各端各建一次。

**与 C2 的依赖关系要说清楚**：本 change 因此**不强依赖** C2 的 `store()` accessor。
C2 仍需先合，理由是（i）两者都改 `crates/transfer/src/store.rs` 与
`crates/storage-sql/src/store.rs`，并行必冲突；（ii）C2 定的「自由函数降 impl」体例是本
change 逐条照抄的模板；（iii）`store()` 对「手里只有 manager」的路径（core 内部将来要访问
收件箱）仍然是必要的出口，本 change 不重复造第二个。

## D6：Web 建真收件箱表——`INBOX_STORE` + `DB_VERSION` 3 → 4

`crates/web/src/idb.rs` 加第四个 object store：

```rust
/// 收件箱 store（key = inbox item uuid 字符串）。
pub const INBOX_STORE: &str = "inbox";
```

三处必须同时改，漏一处就是运行时才暴露的静默失败：

1. `DB_VERSION` 3 → 4（`idb.rs:29`）——不提版本 `onupgradeneeded` 不触发；
2. `install_upgrade_handler` 里 `for name in [KV_STORE, SESSION_STORE, INVITE_STORE]`
   （`idb.rs:142`）加进去；
3. `idb.rs:27` 那行版本沿革注释补 v4，`:3-7` 的「两个 object store」表述改成四个。

分 store 而不是塞进 `kv`，理由与 `sessions` 当初完全一样（`idb.rs:4-7`）：
要能一次 `getAll()` 取回全部条目而不捞到无关记录，浏览器没有便宜的 key 前缀扫描。

**存储形态**：一条 inbox item 一个 key，value 是
`{ item: entity::inbox_item::Model, files: Vec<entity::inbox_item_file::Model> }` 的 JSON，
用与 `crates/web/src/store.rs:544-604` 完全相同的 serde **remote derive** 手法
（`entity` 的 `Model` 是纯 scalar、没有 serde derive，remote derive 让 entity 加列时
**编译期**失败而不是运行期静默丢字段）。复用同一套体例，不发明第二种。

**收件箱不参与 `HISTORY_CAP` 淘汰**（`store.rs:62` 的 100 条上限只作用于 `sessions`，
`prune()`（`:173`）也只扫 sessions map）。这正是建真表最实质的收益：桌面上
「清空传输历史不动收件箱」是有测试钉着的不变量（`crates/storage-sql/src/inbox.rs:797`
的 `clear_history_should_keep_inbox_records`），Web 端在投影方案下**做不到**。

## D7：存量 IndexedDB 数据直接丢弃——不回填、不迁移、不双写、不留兼容层

**结论：不做任何迁移或回填代码。** 升 `DB_VERSION` 只为让 `onupgradeneeded` 建出新 store；
老库里已经存在的「终态 + Completed 接收会话」不会被自动补出收件箱条目。
落地后第一次打开 Web 应用，收件箱是空的。

理由（项目负责人明确指示，此处作为决策记录）：

- **Web 端目前没有真实用户**，「保住旧数据」这件事的收益是零。
- 一切以**架构最合理、最简洁**为准。任何只有在「要照顾存量数据」这个前提下才成立的设计，
  在本 change 里都应当被换成更简洁的那条。回填 / 双写 / 兼容层是纯粹的负债——
  它们不表达任何业务规则，只表达「我们曾经用过另一种存法」。
- 具体到实现路径上，它还会引入一个纯属自找的排序约束：回填必须发生在
  `prune()`（`store.rs:121`）**之前**，否则被 `HISTORY_CAP` 淘汰的会话就回填不到。
  不做回填，这条约束连同它的注释一起不存在。

**要与「实现 `repair_*` 端口方法」区分开**，这两件事看起来像，但不是一回事：

| | 加载期自动回填 | `repair_missing_inbox_items_for_completed_receives` |
|---|---|---|
| 触发 | 隐式，每次 `load()` | 显式，用户/宿主调用（桌面已有 `repair_missing_inbox_items` 命令、移动已有同名 uniffi 方法） |
| 目的 | 兼容旧存储格式 | 修复「`ensure_*` 当时写失败」（`receiver.rs:709` 只记日志不阻断，所以这个洞是**长期存在**的，不是历史遗留） |
| 本 change | **不做** | **做**——它是端口的 10 个方法之一，三端都要实现 |

也就是说：Web 侧照样实现 `repair_*`（否则端口就缺一块），但**没有任何代码在启动时调用它**。
它是一条按需的修复通道，不是迁移。

**若将来 Web 端有了真实用户**，需要的也不是把这段代码补回来，而是在那时按当时的
schema 变更单独评估——不为一个还不存在的场景预留机制。

## D8：Web 没有 FTS——线性子串扫描，语义与 SQL 侧对齐

`search_inbox` 在 SQL 侧是 `inbox_fts` 虚拟表 + `LIKE ... ESCAPE '\'`
（`crates/storage-sql/src/inbox.rs:350-375`，刻意不用 `MATCH`/bm25，因为 trigram 对 2 字中文词
失配——`search_cjk_two_char_word_matches`（`:892`）这条测试就钉着「合同」必须命中）。
浏览器侧没有等价物，也不值得引入 wasm 版 SQLite（把一整个 SQLite 编进 wasm，
只为搜几十条本地记录）。

Web 实现：内存里对每条条目算 `inbox_files_text`，调 D2 的 `inbox_matches` 判命中，
`inbox_snippet` 生成片段，过滤 `deleted_at` 非空、按 `include_archived` 过滤 `archived_at`，
按 `received_at` 倒序、截断到 `limit`。**命中集合、排序、片段三样与 SQL 侧逐条对齐**——
这是 spec 里写死的要求。

规模合理性：Web 收件箱条目数与用户实际接收次数同阶（几十到几百），
线性扫描的成本远低于一次 IndexedDB 往返。

## D9：Web 的 `InboxItemFileEntry.id` = 条目内序号

`InboxItemFileEntry.id: i32` 在 SQL 侧是 `inbox_item_files` 的自增主键。
Web 没有自增，也不该为此造一个全局计数器（那要额外一个持久化的单调计数，
且跨标签页并发时不安全）。

用**条目内序号**（0..n，写入时确定、之后不变）。全局不唯一，但配合 D4 的
`(item_id, file_id)` 双参数完全够用——而 D4 本来就是移动端已经在做的、更安全的形态。

同一理由适用于 `InboxItemFileEntry.transfer_file_id`：Web 直接填对应
`entity::transfer_file::Model.file_id`（同样是会话内序号），语义一致。

## D10：`PersistentSessionStore` 改名 `WebTransferStore`，inbox 实现拆到 `crates/web/src/inbox.rs`

两件小事，一起做：

- **改名**：它实现的是 `SessionStore + InboxStore`，加上真收件箱表之后「SessionStore」这个名字
  就是个谎。改成 `WebTransferStore`——对齐它满足的合并端口 `TransferStore`。
  调用点只有 `crates/web` 内 6 处（`store.rs:41 / :72` 定义与 impl，
  `node.rs:41 / :139 / :175`，加上 `:182 / :219 / :250` 三处 `session_store` 变量名可保留）。
- **拆文件**：`store.rs` 现在 679 行，收件箱实现 + 持久化 DTO 还要加约 250 行。
  新建 `crates/web/src/inbox.rs`，里面放一个**自包含**的 `WebInboxTable`
  （自己的 `Mutex<HashMap<Uuid, StoredInboxItem>>` + 全部 IndexedDB 读写），
  `WebTransferStore` 持有它作为字段，`impl InboxStore for WebTransferStore` 逐条委托。

拆法刻意不是「把 impl 块放到另一个文件里去改同一个 struct 的字段」——那种写法两个文件
共同拥有一份可变状态，读起来要来回跳。`WebInboxTable` 拥有自己的状态，
唯一需要跨界的是 `ensure_*`（要读会话行与文件行），那条把它们**作为参数传进去**
（`inbox.ensure_from_session(&session, &files)`），依赖方向单向。

`repair_missing_inbox_items_for_completed_receives` 因此实现在 **`WebTransferStore`** 上
（它同时握着 sessions map 与 inbox 表），而不是 `WebInboxTable` 里——后者看不见会话。

`crates/web/src/lib.rs` 加 `#[cfg(wasm_browser)] mod inbox;`（与其余模块同门控）。

## D11：移动端 open / show / export 不补 uniffi

立项描述把它列为三条缺口，核实后**前两条不成立、第三条不该做**：

- **open / show**：`mobile/src/app/inbox/[itemId].tsx:221` 起的 `openFile` 已经是
  「先 `openFileWithSystem`（iOS QuickLook / Android 系统应用）→ 打不开就降级到系统分享面板」，
  `:200-216` 的 `shareFile` 走 `shareFileWithSystem`。两者都在失败时调 `markFileMissing`
  （即 D4 那个端口方法）。这条路径**比桌面还完整**——桌面只有「打开失败就报错」，没有降级。
- **export（导出到指定目录）**：桌面 `export_inbox_item` 的语义是「复制到用户选的目录」
  （`src-tauri/src/commands/inbox.rs:107-127` 的 `tokio::fs::copy` 循环）。
  iOS 没有用户可见的文件系统目录概念，Android 上等价动作是 SAF 目录选择器；
  两者的用户心智都是**系统分享面板**，而那条路已经有了。补一条 `export_inbox_item` uniffi
  是把桌面的文件管理器心智搬到不存在文件管理器的平台上。

**留在本 change 里的移动端工作**只有调用点切换：`mobile-core/src/inbox.rs:8` 的
`use swarmdrop_storage_sql::inbox as inbox_ops;` 换成端口，9 个方法改成经 store 调用，
5 个 `From` impl 的来源类型换 crate（穷尽解构的 drift guard 保持），
`:320-328` 的归属校验删除（已由 D4 上提到端口）。uniffi 的**签名形状不变**，
故不需要重跑 `pnpm --filter react-native-swarmdrop-core build:ios`。

## D12：wasm 三条硬约束——本 change 是否触碰

| 约束 | 是否触碰 | 说明 |
|---|---|---|
| **`crates/core` 零 sea-orm** | **不触碰** | `crates/core/src/` 全文没有任何 `inbox` 引用（`grep -rn inbox crates/core/src` 零命中），本 change 一行都不改 core 的 `src/`。只改 `crates/core/tests/e2e_transfer.rs:537` 一处测试调用点 |
| **`crates/transfer` 零 network 依赖** | **不触碰** | 新增的 `inbox.rs` 只用 `entity`（两个普通 enum）、`uuid`、`blake3`、`serde`，以及已在本 crate 的 `TransferProjection` / `TransferOrigin`。零 `swarmdrop-net` 引用 |
| **`crates/invite` 零 core 依赖** | **不触碰** | 本 change 完全不碰 `crates/invite` |

**额外的、本 change 最需要盯的一条**：`crates/transfer` **零 sea-orm 关系类型**。
`entity` crate 本身依赖 sea-orm，但 transfer 已经在用 `entity::TransferDirection` 等普通 enum、
和 `entity::transfer_session::Model` 这种**纯 scalar** 结构——这些 wasm 可编
（`entity` 的 `sqlite` feature 不开时不引入 sqlx/tokio 链）。真正的地雷是 `ModelEx`：
它是 `HasMany` / `HasOne` 的宿主（见 `crates/entity/src/inbox_item.rs:14 / :31`），
一旦出现在端口签名上就把关系类型带进 transfer。

所以新端口的**硬性纪律**：

- ✅ 允许：`Uuid`、`i32` / `i64` / `bool` / `String` / `usize`、`entity::InboxSourceKind`、
  `entity::InboxContentKind`、`InboxItemSummary` / `InboxItemDetail` / `InboxSearchHit` /
  `InboxHitFile` / `InboxItemFileEntry`、`TransferProjection`
- ❌ 禁止：`entity::inbox_item::ModelEx`、`entity::inbox_item_file::ModelEx`、
  `entity::transfer_file::ModelEx`、任何 `sea_orm::*`

`From<entity::inbox_item::ModelEx> for InboxItemSummary` 这类转换**留在
`crates/storage-sql/src/inbox.rs`**（`:92` / `:107` 现址），跟着实现走。

**触碰的 crate 清单**：`crates/transfer`、`crates/storage-sql`、`crates/web`
→ 三个里有两个在 wasm 门禁范围内，`./scripts/check-wasm.sh`（含 `--clippy`）是必过项。

## D13：Web 前端本 change 只做「数据源切换」

`crates/web` 侧给全 10 个端口方法与对应的 wasm 导出，但 `docs/app/app` 只改一处：
`_components/receive-panel.tsx` 的 `InboxPanel`（`:113`）从
`Object.values(projections).filter(...)`（`:119-125`）换成读真收件箱条目。

不做的：检索框、归档按钮、删除按钮、详情页。理由是它们各自要定交互与空态，属于 Web 应用区的
UI 决策，塞进一个存储层 change 会让 review 面变成两块不相干的东西。端口与导出先就位，
UI 后续增量。

**wasm 导出清单**（`crates/web/src/node.rs`）：
`inbox_items(include_archived)` / `inbox_item(item_id)` / `inbox_item_by_session(session_id)` /
`search_inbox(query, limit, include_archived)` / `mark_inbox_item_opened(item_id)` /
`archive_inbox_item(item_id, archived)` / `delete_inbox_item(item_id)`。

**刻意不导出** `mark_inbox_item_file_missing` 与 `repair_missing_inbox_items_for_completed_receives`：

- 前者：浏览器侧没有「文件被外部移动/删除」这回事——OPFS 只有本 origin 能写；真正会发生的是
  **配额驱逐**，那会让整个 OPFS 目录一起消失，逐文件打 `missing` 标记没有意义。
- 后者：它的用途是修复「`ensure_*` 当时写失败」，而 Web 端此刻没有承载它的 UI。
  端口实现照做（三端同构），导出等有调用方再加——**不预留死接口**。

数据刷新时机：挂载时拉一次；`transferCompleted` 事件（`direction=receive`）到达时重拉。
不做订阅式推送——收件箱是低频写入，一次 IndexedDB `getAll` 的成本远低于新造一条事件通道。
**注意 `docs/app/app/_lib/create-store.ts` 是自研 store，selector 里禁止派生新数组/对象**
（`pnpm check:zustand-access` 只扫仓库根 `src/`，不覆盖 `docs/`，这里没有机器兜底），
故收件箱列表存成 store 里的稳定引用，**排序在写入时做完**。

## D14：两套 bindings 都要再生，但预期形状不同

- **桌面 `src/lib/bindings.ts`**：DTO 只是换了 crate（`swarmdrop_storage_sql::inbox`
  → `swarmdrop_transfer::inbox`），specta 的 TS 类型名与字段全不变；命令的 `State<'_, ...>`
  参数本就不进 bindings。**预期 diff 为空**——若非空，说明有字段在搬迁中被改动，要回头查。
  `src-tauri/Cargo.toml` 里 `swarmdrop-storage-sql` 的 `specta` feature 已把
  `swarmdrop-transfer/specta` 带上（`crates/storage-sql/Cargo.toml` 的 feature 定义里写着），
  构建时确认一次即可。
- **`crates/web/bindings/bindings.ts`**：**会变**（新增 5 个收件箱类型）。
  `crates/web/src/types.rs` 按 `pub use swarmdrop_host::device::Device;`（`:11`）的既有体例
  re-export 收件箱 DTO，`crates/web/src/lib.rs` 的 `pub use types::{...}` 清单同步，
  `crates/web/tests/specta_export.rs:56-64` 的 `Types::default().register::<...>()` 链补上，
  跑 `cargo test -p swarmdrop-web --features specta --test specta_export` 再生。
  然后 `docs/` 下 `pnpm build:wasm` 重新生成 `docs/packages/swarmdrop-web/`。

  （顺带：`specta_export.rs` 的模块注释说产物在 `static/types/bindings.ts`，实际是
  `crates/web/bindings/bindings.ts`（`:72` 的 `concat!(env!("CARGO_MANIFEST_DIR"), "/bindings/bindings.ts")`）。
  改这个文件时**顺手把那句注释修正**，别再传下去。）
