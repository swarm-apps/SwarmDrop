# 存储抽象（把 sea-orm 从 core 摘出去）

> **状态：已落地。** 本文是 2026-07-17 的**调研快照**，记录的是切割方案的推导过程。
>
> 落地结果：
> - `crates/core` **零 sea-orm 依赖**
> - 端口 trait（`SessionStore` / `InboxStore`）在 `crates/transfer/src/store.rs`；
>   两者**都已补全** —— `SessionStore` 覆盖运行时写路径 + 历史管理，`InboxStore` 覆盖
>   收件箱的全部 10 类操作，宿主经 `TransferManager::store()` 取回（见「端口覆盖范围」一节）
> - 收件箱的**领域规则**（标题 / 内容指纹 / 聚合文本 / 来源分类 / 命中判据 / 片段）
>   住在 `crates/transfer/src/inbox.rs`，各存储实现调它，不各写一份
> - SQL 实现独立成 **`crates/storage-sql`**（native-only），宿主在组装点注入
> - `crates/entity` 的 sea-orm 已 feature 解绑（Web 端可只吃类型宏）
> - Web 端走 IndexedDB（会话表 + **独立的收件箱表**）+ OPFS，不依赖 storage-sql
>
> **读法**：以「切割线为什么划在 `DatabaseConnection` 而不是 `entity`」「SendWrapper 为何
> 免改 trait 签名」这类判断依据为主；文中「第 0 步 / trait 层未做」等进度描述已过时。
> 当前架构以 `CLAUDE.md` 为准。

## 概览

2026-07-17 调研「把 SQLite 存储抽象成 trait，让 Web 端也能实现断点续传」。

方法：agent 源码级调研 + 本地编译探针。**凡标「实测」的都是真跑过 `cargo check`**，
且本文件里两条最反直觉的结论（entity 能编 wasm、SendWrapper 免改 trait）都由主线独立复核过 ——
它们分别推翻了 agent 和我自己的初始判断。

网络侧的结论在 [libp2p-wasm.md](libp2p-wasm.md)，两份不重叠。

> **当前状态（2026-07-31 更新）：Web 端 IndexedDB 已覆盖 `SessionStore` + `InboxStore`。**
> `crates/web/src/store.rs` 的 `WebTransferStore`（原名 `PersistentSessionStore`，
> 加了真收件箱表之后「SessionStore」这个名字就是个谎，改名对齐它满足的合并端口
> `TransferStore`）= 内存读缓存 + IndexedDB 写穿；收件箱表拆在
> `crates/web/src/inbox.rs`（`WebInboxTable`），低层读写仍收在 `crates/web/src/idb.rs`。
> 本文预判的两条都成立：**trait 签名一个字没改**，`SendWrapper` 裹 JsFuture 满足 Send；
> **entity 的 `Model` 直接上签名**、wasm 编译无碍。
> 四条本文没预判到的实测细节见下方「Web 侧落地实测」。
>
> **（2026-07-19）sea-orm 已彻底摘出 core（openspec: core-wasm-ready）。**
> Sql 实现（`SqlSessionStore`/`ops`/`inbox`）整体搬到独立 crate `crates/storage-sql`
> （swarmdrop-storage-sql，依赖面 = transfer 端口 + entity + host + sea-orm，**不依赖 core**），
> 宿主（src-tauri / mobile-core）在组装点注入。core 同时完成 tokio→n0-future（24 处，
> spawn/time/Instant 换、`tokio::sync`/`select!` 保留），**core 已进 `check-wasm.sh` 六 crate
> 双门常绿**——pairing/presence/device/network 业务域自此 Web 可复用。
>
> （2026-07-18 状态存档）trait 层落地：`SessionStore`/`InboxStore` 端口在 `swarmdrop-transfer`，
> 双 target 可编，transfer 零 `sea_orm`/`DatabaseConnection`。第 0 步（entity 解绑）更早完成。
>
> 落地细节与本文设计的差异：`PeerDirectory`（解 incoming.rs 对配对管理器的依赖）与
> `TransferEventSink`（事件发射依赖倒置，避免 transfer 反依赖 core 的 `CoreEvent`）是设计
> 时未列出的两个端口；宿主端口层（`FileAccess`/`EventBus`/error/device 数据类型）另抽成
> `swarmdrop-host`。`load_resumable_session` 的收编即复用 `SessionStore::find_session`。

---

## 总判决

### 切割线在 `DatabaseConnection`，不在 `entity`

**最反直觉、也最省事的一条。实测：真实 `crates/entity/src` 一个字不改就能编到 wasm32**，
只需把 sea-orm 换成：

```toml
sea-orm = { version = "2.0.0-rc", default-features = false, features = [
    "macros", "with-chrono", "with-uuid", "with-json",
] }
```

（探针 16，`diff -rq` 确认源码与仓库完全一致，`cargo check --target wasm32-unknown-unknown` → Finished，0 error）

对照：`sqlx-sqlite` + `runtime-tokio` 路径撞 mio 硬墙（`error: This wasm target is unsupported
by mio`），`runtime-tokio-rustls` 撞 ring 的 C 代码。

⇒ **entity 的 `Model` / `DeriveActiveEnum` 是 wasm 可用的普通数据类型，不需要为了 Web 端把它们
从 trait 签名里藏起来。** trait 上可以直接用 `entity::*::Model`。

这直接决定了 `host.rs` 里 `CoreSaveLocation` 那套 From 双向转换范式**不该推广到 8 个 enum** ——
那是纯洁癖、零 wasm 收益。它只在真正需要脱钩的地方用（比如公共 API 上的语义类型）。

### `#[async_trait]` 的 `Send` 约束不用动，`host.rs` 现有 6 个 trait 一个都不用改

浏览器里一切（IndexedDB / OPFS / fetch）都经 `wasm_bindgen_futures::JsFuture`，它是 **`!Send`**
（内部持 `JsValue`）。直觉是「trait 的 Send 约束要 cfg 条件化」—— **实测有更省的路**。

**推荐做法**：trait 保持与 `host.rs` 逐字同构（`#[async_trait]` + `: Send + Sync`），
Web 实现内部用 `send_wrapper::SendWrapper` 把 JsFuture 裹成 Send：

```rust
#[async_trait]
impl SessionStore for BrowserStore {
    async fn load(&self, key: &str) -> Option<Vec<u8>> {
        SendWrapper::new(some_js_future()).await   // ← !Send future 在这里被裹住
    }
}
```

实测（探针 15）：wasm32 编译通过，且特意用一个返回 `impl Send` 的 spawn 形状去逼 Send 约束 ——
也过，**是真满足不是绕过**。**core 零改动，认知成本为零。**

⚠️ **代价必须写进实现顶部的注释**：`SendWrapper` 跨线程 drop/access 会 panic。
wasm32 不开 atomics 时是单线程，永不触发；**一旦启用 wasm threads（atomics + shared memory）
就变成活雷**。CI 里要钉住 target 不带 `+atomics`。

### 耦合面比想象小得多

| 项 | 数字 | 出处 |
|---|---|---|
| 以 `db: &DatabaseConnection` 为首参的 pub async fn | 31（`ops.rs` 21 + `inbox.rs` 10）| 已复核 |
| 持有 `Arc<DatabaseConnection>` **字段**的结构体 | **3** | 已复核 |
| 跨表事务 | **1 处**（`inbox.rs:202` `begin()` → `:258` `commit()`），整个包在单个 pub fn 内 | 已复核 |
| 断点续传实际依赖的操作 | 11 / 31，全在一个聚合根内 | ⚠️ 未复核（agent 结论，动手前自己追一遍）|

三个推论：

1. **注入点只有 3 处** → `Arc<DatabaseConnection>` 换 `Arc<dyn SessionStore>` 是 3 行改动：
   - `transfer/manager.rs:145` `pub(crate) db: Arc<DatabaseConnection>`
   - `transfer/coordinator.rs:331` `db: Arc<DatabaseConnection>`
   - `transfer/actor/receiver.rs:58` `db: Arc<DatabaseConnection>`

   （`transfer/actor/sender.rs:295` 只在参数上收 `&DatabaseConnection`，不持有。）

   **但动 trait 前要先清理一处**：`transfer/flow/resume/mod.rs:479` 的 `load_resumable_session`
   绕过 ops 层直连 ORM —— 不收编它，trait 抽了它还在直连。

2. **trait 上永不出现 `begin` / `commit`**。全 core 唯一的跨表事务在 `database/inbox.rs:202-258`，
   整个包在一个 pub fn 内。⇒ 只要 trait 取「用例级」粒度（`create_session` 而非
   `insert_session_row` + `insert_file_row`），事务就是实现细节。

3. ~~**`InboxStore` 可以先不实现**~~（**2026-07-31 已推翻**，见「端口覆盖范围」一节的
   收件箱那半）。当时的理由是：core 内唯一调用点是 `transfer/actor/receiver.rs:657`，
   而 `receiver.rs:656` 的注释已写明「失败只作为 DB 附加错误上报，**不回滚已完成传输**」——
   现成的降级点。这条**只成立于「收件箱只有一个写入口」的时候**：它算漏了列表 / 检索 /
   归档 / 软删这些**管理类**读写，那些没有降级点，不实现就是 Web 端根本没有收件箱。

### 第 0 步（零风险，无论 Web 走哪条路都该做）

`crates/entity/Cargo.toml:7` 的 `sea-orm = { workspace = true }` 硬绑了根 `Cargo.toml:21-27` 的
`runtime-tokio-rustls` + `sqlx-sqlite`。**Web 端只要依赖 entity 就会撞 mio 编译失败。**

改成：

```toml
# crates/entity/Cargo.toml
[dependencies]
sea-orm = { version = "2.0.0-rc", default-features = false, features = [
    "macros", "with-chrono", "with-uuid", "with-json",
] }

[features]
# 桌面 / 移动开启；Web 不开。
sqlite = ["sea-orm/runtime-tokio-rustls", "sea-orm/sqlx-sqlite"]
```

实测已验证改完即过。**不做这步，后面所有 trait 设计都跑不到 wasm 上。**

---

## ❌ 被推翻的旧认知

**这一节记的是调研中被证伪的说法。重新捡起其中任何一条都会导致错误决策。**

### 「先看 sea-orm 能不能编到 wasm」—— 问错了问题，浏览器里根本没有 SQLite 能用

**障碍不是编译，是没有 libc。** 实测（探针 10，`libsqlite3-sys` + `bundled`，已配 brew llvm）：

```
sqlite3/sqlite3.c:14678: fatal error: 'stdio.h' file not found
```

`wasm32-unknown-unknown` **连 libc 都没有**，更不用说 VFS 需要的 POSIX 文件 API 和 `fcntl` 文件锁。
官方 sqlite-wasm 能跑是因为走 **Emscripten**（垫一层 libc + 把 VFS 映射到 OPFS），那是另一条工具链，
`libsqlite3-sys` 到不了。sea-orm 两个 driver 都在它底下。

⇒ **存储 trait 是必需项，不是「优雅」。** 且 Web 侧的实现只能是 IndexedDB / OPFS 一类，
不会是「换个 sqlite 后端」。

### 「sea-orm 2.0 的 `rusqlite` feature 是 wasm 的出路」—— 是半成品，原生也编不过

`sea-orm-2.0.0-rc.43/src/driver/rusqlite.rs:1-10` 有显式 wasm 分支（wasm 上 `web_time::Instant`
替 `std::time::Instant`），`Cargo.toml:664` 还为 wasm target 声明了 `web-time` 依赖 ——
**看起来像是有意支持 wasm，很有迷惑性**。

实际：`Cargo.toml:114` 写着 `rusqlite = []`（空 feature），它 gate 住的 `driver/rusqlite.rs:20`
却 `use sea_query_rusqlite::{...}` —— 而这个 crate **整个依赖表里都没声明**
（全文件 grep `rusqlite` 只有 `:114` 那一行）。

**探针 9 已验证：开这个 feature 编原生 target 一样挂**（`unresolved import sea_query_rusqlite`）。
跟 wasm 无关，就是个没做完的 feature。

### 「必须把 entity 类型全部藏到 `Core*` 镜像类型后面，Web 才能用」—— 错

见「总判决」第一条。实测 entity 原样过 wasm32。把 8 个 enum 都做一遍 `CoreSaveLocation` 那样的
From 双向转换是**纯洁癖、零 wasm 收益**，还平白增加维护面。

### 「`#[async_trait]` 的 Send 约束必须 cfg 条件化，`host.rs` 6 个 trait 全要改」—— 错

这是**主线自己踩的坑**，记在这里防止重犯。

cfg 别名方案（`MaybeSend` / `MaybeSync` + `#[cfg_attr(target_arch="wasm32", async_trait(?Send))]`）
确实能跑（实测两端都过），**但它要求改掉 `host.rs` 现有全部 6 个 trait**，因为 supertrait 也必须
一起条件化 —— 只 cfg `#[async_trait]` 而把 `Send + Sync` 留在 trait 头上是不够的。

`SendWrapper` 方案（见「总判决」第二条）达成同样效果，**trait 签名一个字不改**。

### 「`?Send` 是自然选择」—— 错，会病毒式传染

`#[async_trait(?Send)]` 会沿调用链传染，击穿 core 的 `tokio::spawn`（22 处，
见 [libp2p-wasm.md](libp2p-wasm.md) 的用量盘点）。**不要用。**

---

## trait 设计

### 按聚合根拆，不要一个大 trait

31 个方法拆成两个：

| trait | 方法数 | Web 优先级 |
|---|---|---|
| `SessionStore` | ~21（`ops.rs`）| **必需** —— 断点续传只依赖其中 11 个 |
| `InboxStore` | ~10（`inbox.rs`）| 可先不实现（有现成降级点）|

### 粒度取「用例级」

`create_session` 而非 `insert_session_row` + `insert_file_row`。这样：
- 事务是实现细节，trait 上不出现 `begin`/`commit`
- SQLite 用 sea-orm 事务，IndexedDB 用它自己的事务窗口，语义差异被挡在实现里

### 签名照抄 `host.rs`

```rust
#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn create_session(&self, input: CreateSessionInput<'_>) -> AppResult<()>;
    async fn find_session(&self, id: Uuid) -> AppResult<Option<entity::transfer_session::Model>>;
    //                                                        ^^^^^^ entity 类型可以直接上，实测能编 wasm
    async fn update_file_checkpoint_ranges(&self, ..., ranges: &[(u64, u64)]) -> AppResult<()>;
    // ...
}
```

与既有 6 个 host trait 完全同构。错误类型沿用 `AppResult`。

### 端口覆盖范围：运行时写路径 + 历史管理 + 收件箱，缺一不可

**（2026-07-31 更新，openspec: `transfer-store-port-completion` + `inbox-store-port-completion`）**

trait 抽出来之后，覆盖的其实只有一半：建会话、写 checkpoint、落 outboard、按状态机
转换持久化 —— 全是**运行时写路径**。**历史管理**那六类（列表投影 / 删单条 / 清空 /
取源路径 / 过期回收 / 标记 origin）整组留在 trait 之外，靠宿主直接打 `storage-sql` 的
自由函数。后果是 Web 端补到一半就补不下去：它能续传，却删不掉一条历史（issue #104）。

六类现已全部进 `SessionStore`，端口不再有「一半在 trait 外」的状态。两条配套纪律：

1. **宿主取回 store 的唯一正路是 `TransferManager::store()`**（accessor 与 `file_access()`
   同形，返回 `&Arc<dyn TransferStore>`）。宿主不得再另存一份 ORM 连接做传输查询 ——
   那份影子副本正是 accessor 缺席时被逼出来的，推导见
   [rust-backend.md 的「端口要有出口」](rust-backend.md)。
2. **带生命周期语义的删除走域方法，不走端口。** `TransferManager::delete_session()`
   先用共享判据 `transfer::store::is_deletable`（terminal | suspended）挡住进行中的会话，
   再委托 `store().delete_session()`。守卫放存储实现里会重复两份、且存储层不该持有生命周期
   策略；放各端 UI 则 MCP 与 wasm 导出都绕得过。accessor 因此只服务**纯读**与
   **无生命周期语义的写**（`update_session_origin`）。

顺带一条签名纪律：端口方法一律纯 DTO，`#[sea_orm::model]` 生成的 `ModelEx`
（带 `HasOne` / `HasMany` 关系字段）**不许上 trait** —— 它会把 sea-orm 的关系机制拖进
必须编 wasm 的 crate。`ModelEx → DTO` 的转换发生在 `SqlSessionStore` 的方法体内部。
纯 scalar 的 `Model` 不受此限（见「总判决」第一条），`content_root_of` 的签名改吃
`&[transfer_file::Model]` 之后，Web 侧那份「为避开 `ModelEx` 而抄的同语义副本」直接删掉了。
**这条破功只在 wasm target 上暴露**，`cargo check --workspace` 抓不到 ——
每个 Rust 阶段结束都要跑一次 `./scripts/check-wasm.sh`，别留到最后。

**桌面的 `DatabaseConnection` State 已收窄到两处**：`database.rs` 的连接初始化，以及
`lifecycle.rs` 里给 `SqlInviteStore` 用的那一份。传输与收件箱的读写一条不剩地走端口。

#### 收件箱那半：`InboxStore` 1 → 10

`InboxStore` 一度只有 `ensure_inbox_item_for_completed_receive_session` 一个方法，
另外 9 类操作（列表 / 检索 / 详情 / 按会话查 / 标已读 / 归档 / 软删 / 标文件缺失 / 修复）
是 `crates/storage-sql/src/inbox.rs` 里的 pub 自由函数，桌面、MCP、移动三端各自直调。
病灶与历史管理那半完全同型：**端口比底下的能力弱，于是宿主绕过端口**。

补全时定下的四条，都不止适用于收件箱：

1. **端口不能比底层能力弱，否则复用会被逼成复制。**
   `ensure_inbox_item_for_completed_receive_session` 原本返回 `()`（SQL 实现用 `.map(|_| ())`
   把 `Option<InboxItemDetail>` 丢掉），于是 `repair_missing_inbox_items_for_completed_receives`
   **没法用端口的 `ensure_*` 实现**，只能在每个存储实现里各写一遍「查重 + 构造条目」。
   改成返回 `Option<InboxItemDetail>` 之后，两端的 repair 都是「循环里调 ensure」。
   判据不是「信息更全更好」，是**「弱返回值会让上层能力没法用下层能力搭出来」**。
2. **同一条安全检查只有一个宿主做，就说明它该进端口。**
   `mark_inbox_item_file_missing` 原签名是 `(file_id, missing)` —— 靠 `inbox_item_files`
   自增主键全局唯一。移动端桥接层自己补了归属校验（断言 `file_id` 属于该条目），桌面没有。
   端口签名改成 `(item_id, file_id, missing)`，校验搬进实现，移动端那段删掉。
   顺带解决 Web 的死结：IndexedDB 没有自增主键，文件 id 只能是**条目内序号**，
   全局不唯一，单靠 `file_id` 在浏览器上根本定位不到。
3. **领域规则住 `crates/transfer`，各存储实现调它**（下一小节展开）。
4. **管理类语义也是端口的一部分。** `crates/transfer/src/store.rs` 的模块头曾把端口描述成
   「运行时写路径」，那是漏了半张脸：列表 / 检索 / 归档 / 软删同样是端口契约。
5. **端口方法收确定值时，每个宿主都会自己发明一个默认值。**（#111）
   `search_inbox(query, limit: usize, ..)` 收的是确定的 `usize`，于是四个宿主想出了四个答案
   （Tauri 命令 20、桌面 MCP 20、移动 100、Web 50）。**注意「四个」里有两个在同一个进程**
   ——同一台机器上「用 UI 搜」与「让 AI 搜」结果集不同，这种同宿主内的第二事实源比跨端分叉
   更难发现。
   修法是加一个 provided 方法把兜底收进端口：

   ```rust
   async fn search_inbox_capped(&self, q: &str, limit: Option<u32>, inc: bool) -> AppResult<..> {
       let limit = limit.map_or(INBOX_SEARCH_LIMIT, |n| (n as usize).min(INBOX_SEARCH_LIMIT));
       self.search_inbox(q, limit, inc).await
   }
   ```

   要点有两个，缺一条都白做：**让 `Option` 成为宿主面对的类型**（「自带一个默认值」那条路
   在类型上就不存在了）；**上限只能收窄不能放宽**（否则分叉只是从常量搬到了参数上）。
   判据：端口参数如果有「合理默认值」，那个默认值就该在端口里，不该让每个宿主猜。

#### 领域规则住 `crates/transfer`，各存储实现调它

补全过程中最有复用价值的一条体例。`storage-sql` 的收件箱实现里混着两类东西：

| | 例子 | 归属 |
|---|---|---|
| **领域规则** | 首文件名取哪个、内容指纹怎么算、检索覆盖哪段文本、片段怎么截 | `crates/transfer/src/inbox.rs`，**各实现共用一份** |
| **SQL 细节** | `escape_like`、`LIKE ... ESCAPE '\'` 的写法、`From<ModelEx>` 转换、FTS 虚表 | 留在 `crates/storage-sql`，跟着实现走 |
| **展示串** | 「空传输」/「X 等 N 个文件」怎么拼 | **各端自己**，见下方「派生展示串」一节 |

分界判据是「**分叉了会不会让同一批数据在两端表现不同**」。
`inbox_content_hash`（blake3 逐文件累加 `relative_path ‖ 0x00 ‖ checksum ‖ size_le`）
是跨端去重的**唯一**判据，字节级不同就等于这个字段作废；`inbox_primary_file_name`
分叉了同一批文件在桌面与浏览器指向不同的文件。这类必须共用。而 `escape_like` 分叉了没人
看得出来 —— 它只是 SQL 通配符的转义。

**「三端各判一遍同一件事」是内核没把话说完的信号**（#111）。`inbox_snippet` 原本在一个候选
都不命中时**回退整个标题**，而三端的条目行上本来就显示着标题——于是桌面无条件渲染那行重复、
移动端判真、Web 在前端比对字符串。后者尤其糟：它编码了 `snippet_window` 的窗口半径与省略号
规则，改内核就静默失效，且没有测试守着。

修法是让内核表达「不该渲染」这个状态本身（返回 `Option<String>`），而不是让每端从产出物
反推。**判据是：如果每端都要写一段「这个返回值要不要用」的逻辑，那段逻辑属于产出它的那一层。**

#### 派生展示串既不该共用，更不该落库（2026-08-04，openspec: `inbox-title-structural`）

上一条说「领域规则必须共用」，但**展示串是例外，且方向相反**——它根本不该在领域层产出。

收件箱条目的标题曾经由 `crates/transfer/src/inbox.rs::inbox_title` 拼成中文散文
（「空传输」/ 文件名 /「X 等 N 个文件」）并**写进 `inbox_items.title` 落库**。三层后果，
一层比一层不显眼：

1. 三端的 Lingui 都够不着它，切语言时收件箱那一栏纹丝不动；
2. 存量条目的语言被**写入时刻**钉死，改生成逻辑也救不回来；
3. 移动端把它当文件名用（`isImageFile(item.title)`）——单文件条目恰好成立，
   多文件条目的扩展名变成「个文件」，于是「收了 3 张图」既进不了「图片」筛选、
   也拿不到对应图标，且**不报错**。

现在领域层只给结构：`primary_file_name: Option<String>`（`inbox_primary_file_name` 取第 0 个）
+ 既有的 `item_count`，展示串由三端各按当前 locale 生成。这与 `localize-backend-strings`
的「后端只发稳定语义码 + 结构化参数，翻译发生在呈现边缘」是同一条原则——`inbox_title`
是那次遗漏的第四桶，也是唯一一桶把散文**持久化**了的。

**三端各写一份三分支，刻意不收进 `packages/shared-view`**：能共享的只有
`itemCount === 0 / === 1 / > 1` 这个判别，真正的内容是文案，而文案本就分属三套独立
catalog（桌面 Lingui 5、Web Lingui 6、移动 Lingui 6）。为省三行引入跨 workspace 依赖不划算。

**`Option` 而不是空串**：「没有文件」与「文件名恰好是空串」必须在类型上分得开，
与 `inbox_matches` 的 `extracted_text: Option<&str>` 同一条纪律。
⚠️ 两套 codegen 对 `Option<T>` 的映射**不同**：specta（桌面 / Web）给 `string | null`，
uniffi（移动）给可选属性 `primaryFileName?: string`。移动端那个 hook 因此收
`string | undefined`（它的调用点全是 uniffi 生成的类型），桌面 / Web 那两份收
`string | null`——这不是可以统一掉的东西。**跨端复制展示逻辑时，类型签名是最后才暴露
问题的地方**：逻辑照抄能跑，`?? ""` 也照样工作，只有 `tsc` 会拦。

##### 由已索引字段派生的展示串，不进检索索引

同一次改动删掉了 `inbox_fts.title` 列，**没有**按原计划另起一个 `search_text` 列。
推导只用到包含关系：设 `T = inbox_title(files)`、`F = inbox_files_text(files)`
（`"{name} {relative_path}"` 逐文件拼接），

| files | `T` 相对 `F` 独有的可搜文本 |
|---|---|
| `[]` | `"空传输"` |
| `[f]` | **无**（`F` 以 `f.name` 开头） |
| `[first, ..]` | `" 等 N 个文件"` |

所以删列后检索行为的变化**恰好两条**：搜「空传输」不再命中零文件条目、搜「个文件」
不再命中全部多文件条目。后者是**修复**——所有多文件条目的标题都含「个文件」，
用户搜「文件」会把它们整批捞出来。

**可复用的判据：一个派生字段该不该进索引，看它相对已索引内容有没有独立信息。**
纯函数派生且有损的投影不可能带来新信息，只可能带来模板噪音。
`storage-sql` 的 `title_template_words_no_longer_match` 钉着这条。

##### 顺带补上的既存缺陷：文件顺序此前没有任何强制

`inbox_content_hash` 逐文件累加，**顺序是字节级契约**（有 known vector 钉着），
`primary_file_name` 取第 0 个也依赖它。但 `crates/storage-sql/src/inbox.rs` 里
`TransferSession::load().with(TransferFile)` 的关系加载**没有任何排序**——整个契约
一直靠 SQLite「不加 `ORDER BY` 时按 rowid 返回」这一实现行为兜着。加 join、改查询计划
或换后端都会静默改掉 `content_hash`（跨端去重的唯一判据），而那是不会报错的一类损坏。

现已在构造 facts 前显式 `file_rows.sort_unstable_by_key(|f| f.id)`，顺序契约也写进了
`InboxFileFacts` 的文档。注意源头是 **`transfer_files`** 而不是 `inbox_item_files`
——后者是按前者顺序插入的产物，所以迁移回填用 `ORDER BY inbox_item_files.id`
与运行时天然一致。

验证这条修得对不对有个便宜的办法：改完之后**桌面与移动前端一行都没动**——它们本来就写
`hit.snippet &&` / `hit.snippet ?`。既有写法与新语义天然吻合，说明 `Option` 才是那个字段
本来该有的形状。

三条配套纪律：

- **共享规则一律吃中立视图，绝不吃 `ModelEx`。** 签名收在
  `InboxFileFacts<'a> { name, relative_path, checksum, size }`，两端各自从自己的行类型构造。
  `From<entity::inbox_item::ModelEx>` 之类的转换**留在 storage-sql**。破这条
  `cargo check --workspace` 抓不到，只有 `./scripts/check-wasm.sh` 会红。
- **规则要带已知向量的单测。** `inbox_content_hash` 在 `crates/transfer/src/inbox.rs` 里钉了
  十六进制串 —— 它是跨端一致性的唯一机器保障，没有它「两端算出同一个哈希」只是口头约定。
- **没法共用的那条，也要有个规范锚点。** `inbox_matches`（大小写不敏感子串）只有 Web 会真调用，
  SQL 侧是数据库里的 `LIKE`。它仍然放在共享模块里，SQL 那段 raw SQL 的注释指向它 ——
  于是「LIKE 必须复刻 `inbox_matches`」有了一个可读、可测的落点，而不是散在两处的默契。

#### 宿主怎么拿到端口：组装点建一次，注入与自持是同一个 `Arc`

上面纪律 1 说「取回 store 的唯一正路是 `TransferManager::store()`」，收件箱把它逼出了一条
**必要的补充**：那条路只适用于「手里已经有 manager」的调用点。

收件箱命令**不依赖节点启动** —— 桌面早早 `app.manage(db)`，而 `NetManagerState` 是
`Mutex<Option<..>>`、`with_manager!` 在未启动时返回 `node_not_started`。改走 `manager.store()`
会让「没联网也能翻已经收到的东西」变成「先启动节点」。收件箱按定义是**与网络无关的内容账本**，
把它绑到节点生命周期上是方向性错误。

所以形态是标准的**组合根**：组装点建一次 `Arc<dyn TransferStore>`，**同一个 `Arc`** 既注入
`TransferManager` 的工厂闭包、也 `app.manage(...)` 给宿主自己用（移动端由 `MobileCore` 持有，
与 `ensure_db()` 同生命周期）。

**这和「宿主另存一份影子副本」不是一回事**，区别在存的是什么：

| | 存的东西 | 判定 |
|---|---|---|
| 反面（此前的桌面 / Web） | `DatabaseConnection` / 具体类型 `Arc<PersistentSessionStore>` | 绕过端口的**第二条路**，是第二事实源 |
| 正面（现在） | **端口本身**，且是**同一个实例** | 组合根的标准形态，没有第二事实源 |

副产品是可度量的：`SqlSessionStore::new(...)` 在生产代码里的构造点从 **4 处降到 2 处**
（桌面 `setup.rs` 一处、移动 `app.rs` 一处），各端各建一次。
**两个组装点都要写明「注入的与自持的是同一个 `Arc`」** —— 那是这条纪律的全部内容，
写成两个包装同一条连接的实例就白做了。

### 两条刻意的平台差异（都不是待补的缺口）

**删记录 ≠ 删文件，三端一致。** `delete_session` / `clear_all_history` 只清账本：

- **不删收件箱条目** —— `inbox_items.transfer_session_id` 是 `ON DELETE SET NULL`
  （`m20260627_000002_drop_inbox.rs`），条目留下、外键置空
- **不删已落盘文件** —— 收件箱是「结果」，传输记录是「过程」，删过程不动结果

Web 不因为「文件在 OPFS 里」就分叉。同一个「删传输记录」在桌面不删文件、在 Web 删文件，
是最糟的那种平台分叉 —— 用户不会读两套文档。（`clear_all_history` 另有一条收窄：
只删 `phase = Terminal`，非终态保留。删掉一个进行中会话的行只会留下仍在写 checkpoint
的孤儿 actor，与「单条不可删」自相矛盾。）

**Web 的过期回收只做 DB 侧，不清残件。** 桌面在 `database.rs` 里按真实路径直接
`tokio::fs::remove_file` 删 `.part`，移动端走 `cleanup_expired_part_files`，Web 两条都不走 ——
因为 Web **没有 `.part` 中间态**，sink 路径就是文件的最终路径，而
`cleanup_expired_part_files` 按会话的**全部**文件元数据重建 sink 再 `cleanup_sink`：
一个「A 已写完、B 只写了一半」的多文件会话会把 A 一起删掉。桌面能这么做，是因为它删的是
`xxx.part`。

回收的**调用时机**倒是三端对齐了：一律排在 `cleanup_recoverable_sessions()` **之后**，
判据统一为 `phase = Suspended`。Web 此前把回收跑在 `WebTransferStore::load()`
（当时还叫 `PersistentSessionStore`）里
（节点起来之前，遗留的 `Active` 还没转 `Suspended`），被迫改用「非终态」判据 ——
调用点一对齐，那条分叉的判据连同它的解释性注释一起消失。**这类分叉的根因往往是时序而不是平台**，
下次看到「某端判据不一样」先查调用点排在哪。

**已知负债（仍未修，`inbox-store-port-completion` 没有捎带它）**：删掉一个 suspended 的
**接收**会话后，OPFS 里的残件成孤儿（DB 行没了，reap 再也扫不到它）。桌面上它躺在用户的
接收目录里、可见可删；Web 上它占配额且用户完全看不见。清理要按「哪些文件真没写完」来做，
与收件箱的文件生命周期（`opfs::remove_path` 已在 `web-cancel-and-invite-preview` 补上）
一并处理。

**相关文件**：`crates/transfer/src/store.rs`、`crates/transfer/src/inbox.rs`、
`crates/transfer/src/manager.rs`、`crates/storage-sql/src/store.rs`、
`crates/storage-sql/src/inbox.rs`、`crates/web/src/store.rs`、`crates/web/src/inbox.rs`、
`crates/web/src/node.rs`、`src-tauri/src/database.rs`、`src-tauri/src/setup.rs`

---

## Web 侧落地实测（2026-07-27，`#81`）

四条本文设计时没预判到的，全部有实现为证（`crates/web/src/store.rs` + `idb.rs`）。

### `SendWrapper` 要连 `JsValue` 的**构造**一起裹进去

本文只写了「裹 JsFuture」，**不够**。`JsValue` 建在 wrapper 外面，它就活在外层 `async fn` 的
frame 里，整个 future 照样不是 `Send`：

```rust
// ❌ 编译不过：future cannot be sent between threads safely
let value = JsValue::from_str(&json);
SendWrapper::new(idb::put(store, &key, &value)).await

// ✅ 值的构造也在 wrapper 里面
SendWrapper::new(async move {
    let value = JsValue::from_str(&json);
    idb::put(store, &key, &value).await
}).await
```

判据很简单：**wrapper 之外不能出现任何 `JsValue`**（包括参数、临时值、返回值）。

### entity 的 `Model` 能上 trait 签名，但**不能直接落库**

本文「entity 原样过 wasm32」的结论只覆盖编译。落库还差一层：`Model` 是
`#[sea_orm::model]` + `DeriveEntityModel`，**没有 serde derive**（sea-orm 侧不需要），
所以持久化得自建 DTO 做 From/Into。

这反而是好事——DTO 是这份存储的 wire 格式显式声明点，改字段就是改格式，而不是让 ORM 的
派生细节悄悄决定磁盘布局。给 entity 加 derive 是另一条路，但 `ModelEx` 带
`HasMany`/`HasOne` 关系字段，跟着一起派生会炸，**不要走这条**。

### 每个操作自开一个 IndexedDB 事务，内部不跨 `await`

本文「地雷」里说的「IndexedDB 事务是微任务窗口内自动提交」在实现上的形态：拿到
`IDBObjectStore` 后直到请求 settle 之间**不能有别的 await**，否则拿
`TransactionInactiveError`。代价是每次操作重开一次连接——Web 的写频率（接收侧 checkpoint
每 10 个 chunk = 2.5 MB 一次）下可忽略，不值得为它引连接缓存 + `thread_local` 的复杂度。

### 「Web 端断点续传」只有**接收方向**成立

本文的出发点是「让 Web 也能断点续传」，实测要打个对折：**发送方向物理上做不到**。
发送侧的文件内容来自用户选中的 `File` 对象，页面刷新后 JS 上下文销毁，浏览器不允许在未经
用户重新选择的情况下再读同一个文件（File System Access 的持久 handle 只有 Chromium 有，
且仍需授权）。所以存储抽象再完美，非终态发送会话恢复出来也只是个点了必失败的续传按钮。

⇒ **落库范围应按「恢复得了吗」筛，而不是「有没有状态」**：终态会话（历史/收件箱）+ 非终态
的接收会话落库；非终态发送会话与待决 offer（`pending_offers()` 是内存态，刷新后无处应答）
不落库。判定见 `store.rs` 的 `worth_persisting`。

## Web 侧收件箱：建真表而不是投影（2026-07-31，`inbox-store-port-completion`）

收件箱在 Web 端一度是「`direction=receive` 且 `terminalReason=completed` 的会话投影」
（`docs/app/app/_components/receive-panel.tsx` 里对 projections 过滤一遍）。
它有一处**结构性**缺陷，不是能靠打补丁修的：

**会话表有 `HISTORY_CAP = 100` 淘汰，收件箱不该有。** 收件箱是**结果账本**，
传输历史是**过程账本**，删过程不动结果 —— 这条在桌面有测试钉着
（`clear_history_should_keep_inbox_records`）。投影方案下 Web 端**做不到**：
过程记录被挤掉，结果就跟着消失。这是建真表最实质的收益，也是判断「投影够不够用」的通用问法：
**被投影的那张表有没有独立于结果的生命周期？** 有，投影就是错的。

落地形态（`crates/web/src/inbox.rs` 的 `WebInboxTable`）：

- **第四个 object store `inbox`**（key = item uuid），与 `sessions` 平级，
  `prune()` 只扫 sessions map，收件箱条目不在其列 —— 这句写死在 `prune()` 的文档注释里。
- **拆文件的方式是「自包含的表」，不是「把 impl 块挪到另一个文件」。**
  `WebInboxTable` 有自己的 `Mutex<HashMap<..>>` 与全部 IndexedDB 读写，不认识会话表；
  唯一需要跨界的 `ensure_from_session(&session, &files)` 把会话行与文件行**作为参数收进来**，
  依赖方向单向。反面（两个文件共同拥有一份可变状态）读起来要来回跳。
  因此 `repair_missing_inbox_items_for_completed_receives` 实现在 `WebTransferStore` 上
  —— 只有它同时握着两张表。
- **没有自增主键，文件 id 用条目内序号**（0..n，写入时定、之后不变）。
  全局不唯一，配合上面那条 `(item_id, file_id)` 双参数够用；造一个持久化的全局计数器
  在跨标签页并发下并不安全，不值得。
- **持久化形态复用 serde remote derive**（`entity::inbox_item::Model` +
  `inbox_item_file::Model`），与 `store.rs` 的 `SessionRowDef` / `FileRowDef` 同一套 ——
  entity 加列时**编译期**失败而不是运行期静默丢字段。不发明第二种存法。

### schema 变更直接换，不写迁移 / 回填 / 双写 / 兼容层

`DB_VERSION` 3 → 4 只为让 `onupgradeneeded` 建出新 store。**老库里已存在的「终态 +
Completed 接收会话」不会被自动补出收件箱条目** —— 落地后第一次打开，收件箱是空的
而传输历史是满的，那是预期结果不是 bug。

判据（项目负责人的明确指示，作为决策记录）：**Web 端目前没有真实用户**，「保住旧数据」
收益为零；回填 / 双写 / 兼容层不表达任何业务规则，只表达「我们曾经用过另一种存法」。
具体到这次，回填还会引入一个纯属自找的排序约束 —— 它必须发生在 `prune()` **之前**，
否则被 `HISTORY_CAP` 淘汰的会话回填不到。不做，这条约束连同它的注释一起不存在。

**要与「实现 `repair_*` 端口方法」区分开**，两者看起来像但不是一回事：

| | 加载期自动回填 | `repair_missing_inbox_items_for_completed_receives` |
|---|---|---|
| 触发 | 隐式，每次 `load()` | 显式，用户/宿主调用 |
| 目的 | 兼容旧存储格式 | 修「`ensure_*` 当时写失败」——`receiver.rs` 只记日志不阻断，这个洞**长期存在**，不是历史遗留 |
| 做不做 | **不做** | **做**，三端都实现；但 Web 侧**没有任何代码在启动路径上调它** |

将来 Web 端真有了用户，需要的也不是把这段代码补回来，而是在那时按当时的 schema 变更单独
评估 —— 不为一个还不存在的场景预留机制。

---

## 地雷

### `entity::TerminalReason` 在 CBOR wire 协议上（跨版本兼容风险）

`crates/core/src/protocol.rs:8` + `:148` 把 `entity::TerminalReason`（SeaORM 的 `DeriveActiveEnum`）
放上了 `ResumeReport`（构造于 `transfer/flow/resume/mod.rs:160-171`）。

**抽存储 trait 时若顺手把 entity 从 core 摘除，该字段的编码可能变** → 老客户端收不了新客户端的
ResumeProbe 应答 → **跨版本续传静默失败**。

改前必须逐字比对新旧类型的 serde 表示（`crates/entity/src/lib.rs:152-161` 带
`#[serde(rename_all = ...)]`）。**建议单独立项，不要混进存储重构。**

### `inbox.rs` 有硬编码的 SQLite 裸 SQL

- `database/inbox.rs:245-256` —— FTS 全文检索写入
- `database/inbox.rs:347-372` —— FTS 检索 + trigram 虚表

两处都硬编码 `DbBackend::Sqlite`。IndexedDB 的事务是「微任务窗口内自动提交」，语义上不兼容
`inbox.rs:202-258` 那种事务内穿插 `await` 的写法。

⇒ ~~又一条「Web 端先不实现 `InboxStore`」的理由~~。**2026-07-31 已按当时那句「真要做，
全文检索得换实现（不是换后端）」落地**：Web 侧不引 wasm 版 SQLite，改成内存线性子串扫描，
判据由 `swarmdrop_transfer::inbox::inbox_matches` 规范定义，SQL 侧那段
`LIKE ... ESCAPE '\'` 是它的复刻。规模上站得住——收件箱条目数与用户实际接收次数同阶
（几十到几百），线性扫描比一次 IndexedDB 往返还便宜。

### `SendWrapper` + wasm threads

见「总判决」第二条。**单线程假设要写死在注释里 + CI 钉住 target。**

---

## 未决 / 待查

### `create_session` 是否原子 —— 未找到证据

`database/ops.rs:102-119`（嵌套 ActiveModel，`add_file` × N 后 insert）由 `#[sea_orm::model]` 宏生成，
在 `sea-orm-2.0.0-rc.43` 的 `active_model_ex.rs` 里 grep `begin|transaction|fn insert` **全无匹配**。

**这条不定，SQLite 与 IndexedDB 两端行为会悄悄分叉。** 建议先实测（构造 file insert 失败的场景，
看 session 行是否残留）再定 trait 契约。

### ~~Web 侧后端选型~~ —— 已定（2026-07-27）

结构化数据（session / checkpoint）走 **IndexedDB**，不引 `idb`/`rexie`，直接 `web-sys`
（`crates/web/src/idb.rs`，约 170 行，身份/配对/会话三处共用）；文件数据走既有的
`FileAccess` trait 的 **OPFS 异步实现**。

原设计里的顾虑成立且已避开：OPFS 的 `FileSystemSyncAccessHandle` 只能在 Web Worker 里用，
而 `webrtc-websys` 在 Worker 里会 panic（见 [libp2p-wasm.md](libp2p-wasm.md)）——所以
`OpfsFileAccess` 全程走主线程 async API，禁用 SyncAccessHandle。

## 第三个走同一模式的端口：`InviteStore`（2026-07-30 落地）

邀请注册表的落盘（openspec: invite-persistence）照 `SessionStore` / `InboxStore` 的路子又走了
一遍，形态完全同构：trait 在 `crates/invite/src/store.rs`（**仍 wasm-clean** —— trait 定义不带
任何存储依赖），native 实现 `crates/storage-sql/src/invite.rs`，wasm 实现
`crates/web/src/invite_store.rs`（`SendWrapper` 裹 `JsFuture` 那条经验直接复用）。

这一例贡献了三条前两个端口没暴露的东西：

### 「落盘只是备份，所以端口不必返回错误」是个陷阱 —— 判据不是权威性，是失败后果

这一节最初写的是：「`InviteStore` 四个方法全部返回 `()`，因为 CAS 在内存里完成、落盘只是
写穿备份，写库失败时正确的用户可见行为是『配对照样成功』」。**判据错了**，写方法现在返回
`bool`。推翻它的是一次审查 + 三个探针，过程值得留着。

错在把「内存是权威」推成了「落盘失败无所谓」。真实的失败后果按路径分三档：

| 写穿失败的操作 | 库里留下 | 重启后 | 判定 |
|---|---|---|---|
| `register` 的 upsert | 没有这行 | 「不认识」→ 拒绝 | benign |
| `try_consume` 的 upsert | `register` 写的 `Pending` | **可再消费一次** | 破一次性语义 |
| `revoke` 的 upsert | 同上 | **撤销失效、邀请复活** | 破用户的显式意图 |

关键是那个反直觉点：**`register` 已经把 `Pending` 写进库了**，所以此后任何一次写穿失败
（UPDATE 也好 DELETE 也好）都留下同一个结果 —— 库里那行还是 `Pending`。我一度以为「撤销
从删行改成写状态」修好了这个，那是错的：两种失败后果完全相同，实测过。

正确的判据：**端口方法要不要返回错误，取决于「失败后果是 benign 还是破坏不变量」，
而不是「这份数据是不是权威」。** 备份的写失败一样能破不变量 —— 只要有别的路径会把那份
备份读回来当事实（这里就是启动 `load`）。

具体怎么改（可复用的形状）：

1. **写方法返回 `bool`**（不是 `Result`）—— 端口层没有统一错误类型时，调用方只需要
   「成没成」这一位，错误详情归实现层的日志。配 `#[must_use]` 免得又被忽略。
2. **在能 fail-closed 的那条路径上真的 fail-closed。** `try_consume` 敢报错是因为调用点的
   顺序：`respond_pairing_request` 里它排在 `responder.send(Success)` **之前**，此刻配对还没
   成功，报失败是诚实的。**这个顺序是能不能 fail-closed 的唯一依据** —— 先看调用点再决定。
3. **不能 fail-closed 的路径要把失败上报到 UI。** 撤销没有可中止的下游动作，但用户必须知道
   「撤销只在本次运行内生效」，否则他以为撤销干净了。
4. **`load` 之类的恢复路径要状态单调** —— 不允许库里的低优先级状态盖掉内存里的终态。
   写穿失败恰好制造出「内存 Consumed / 库 Pending」，重复 load 就复活了。

### 这类失败模式在测试里默认不存在，必须注入

「权威判定在内存 → 释放锁 → await 写穿」这个形态有个**只在这个形态下存在**的性质：内存已改、
库未落地的那段窗口里，别的调用方看到什么。它独立于任何单元语义 —— 单看 `try_consume` 的
输入输出永远看不出来，必须单独钉。

而**默认写法的内存桩钉不到它**：桩的 `upsert` 立刻返回，窗口宽度是零，那个中间态在测试里
从来没存在过。`invite-persistence` 一开始 20 个测试全绿，窗口内的可见性一条没覆盖。

桩要能注入三件事 —— 静默失败、可撑开的窗口、以及**「已进闸」的回执**：

```rust
struct TestStore {
    records: Mutex<HashMap<..>>,
    fail_writes: AtomicBool,                    // 静默丢弃写入
    hold_writes: Mutex<Option<Arc<Notify>>>,    // 挂在栅栏上，把窗口撑开
    entered: Notify,                            // 回执：写穿真的进闸了
}

async fn upsert(&self, record: InviteRecord) -> bool {
    let gate = self.hold_writes.lock().unwrap().clone();  // 先取出再 await，锁不跨 await
    if let Some(gate) = gate {
        self.entered.notify_one();   // 回执必须发在挂起之前，否则测试等不到
        gate.notified().await;
    }
    if self.fail_writes.load(SeqCst) { return false; }
    /* 落盘 */ true
}
```

**回执不是方便，是正确性。** 我第一版用 `tokio::task::yield_now()` 猜时序，两头都能出错：
断言可能跑在 CAS 之前（假红），放行也可能跑在挂起之前——而 `notify_waiters()` **不存许可**，
错过就是永久挂住。两个 `Notify` 都要用 `notify_one()`（存许可，握手不依赖谁先被调度）。

**这条测试要用 `multi_thread` flavor。** 单线程 runtime 下「spawn 的任务被 poll 一次正好撞到
闸」这个巧合会让猜时序的写法也通过，等于把护栏调松。实测：把握手换回 `yield_now()` +
`notify_waiters()`，单线程 30/30 过，多线程 **12/12 永久挂住** —— 同一个缺陷，只有多线程
才暴露。

四个坑：

1. **开闸时机**。`crates/invite` 的闸是无差别的（挂住后续所有写），所以必须**在 `register`
   之后才开闸**，否则注册本身第一个卡死、测试走不到被测状态。若窗口内还会触发别的写穿，
   就得改成按状态/按键选择性挂闸。
2. **回执发在 await 之前**（上面那条）。顺序反了就是双向等待。
3. **断言顺序：先读「用户/库看到什么」，再调会改状态的路径。** 反了会读到 mutator 之后的
   状态、断言假红。凡是「重启后用户看到什么」这类断言，一律排在所有 mutator 前面。
4. **别在测试里重算键**（如 `sha256(capability)`）—— 那是把生产代码的键推导复制一份进测试，
   算法一改测试就悄悄查不到东西，而 `Option` 变 `None` 后断言可能照样过。只放一条记录、
   用 `values().next()` 直查。

值得各钉一条测试的四条性质（都不是单元语义）：

| 性质 | 桩配置 |
|---|---|
| 窗口内的可见性仍收紧 | `hold_writes` |
| 写穿失败后，重启是否放行 | `fail_writes` + 新建注册表 + `load` |
| 重复 `load` 会不会把内存态降级 | `fail_writes` + 同一注册表上再 `load` |
| 两个重叠写入乱序落地的后果 | `hold_writes` + 期间跑另一条 mutator |

后三条都靠「**新建一个注册表、共用同一个 store**」模拟重启 —— 这是这类端口测试的标准动作，
比真起一次 SQLite 便宜得多，且能精确控制失败点。

**「锁不跨 await」有个免费的编译期护栏，但它只属于 native 门禁。** tokio spawn 要求 future
`Send`，`MutexGuard` 跨 await 就 `!Send` → `cargo check --workspace` 直接红，不用为它单独写
测试。反面要记住：**wasm 侧的 future 没有 `Send` 要求**（`spawn_local` 不要），所以只跑
`check-wasm.sh` 是看不出锁跨 await 的。

副产品：`crates/invite` 的 dev-dependencies 因此需要 tokio 的 `sync` feature —— dev-deps
不参与 wasm 编译（`check-wasm.sh` 没有 `--all-targets`），不破门禁。

**什么时候值得建这套桩**：端口方法是 async + 权威判定在内存 + 写穿在锁外，三条同时成立。
只满足前两条（比如同步端口）就不必，那时没有窗口。

### 「锁内改内存 → 释放锁 → await 写穿」是这类端口的固定形态

`std::sync::MutexGuard` 不能跨 `await`（不是 Send），所以异步写穿天然被迫写成：

```rust
let record = {
    let mut table = self.inner.lock().unwrap();
    // …改内存，顺手把要落盘的快照 clone 出来
};                       // ← 锁在这里释放
self.store.upsert(record).await;
```

编译器的这条限制刚好等于我们要的顺序（CAS 先成功、再落盘），**不需要额外的纪律去维持**。

### 落盘失败不回滚内存态：状态宁可比库更严

`InviteRegistry` 写库失败时保留内存里已置换的状态。方向是刻意的：内存说「已消费」而库还是
「待用」，重启后那条邀请会「复活」一次，但它仍要过验签 + TTL，且用户能在列表里看到并撤销；
反过来（库说已消费、内存说待用）才会导致同一邀请被用两次。

顺带一个**被推翻的直觉**：一开始以为「已消费记录不能提前删，否则重启后同一邀请又能用」。
读代码发现注册表是 fail-closed 的（`.ok_or(InviteRejectReason::Unknown)?` —— 查不到即拒绝），
删早了只会让它变「不认识」，不会放行。保留已消费记录到过期的真实理由是 **UX**（让发起方看到
「已被使用」而不是凭空消失）。这个区别决定了将来要压表大小时可以牺牲哪一边。

**相关文件**：`crates/invite/src/store.rs`、`crates/storage-sql/src/invite.rs`、
`crates/web/src/invite_store.rs`、`crates/migration/src/m20260730_000001_pair_invites.rs`

## 「删已落盘文件」曾是三份编排 + 一处绕过端口（2026-08 已收口）

> **状态：已修（#111 的后续）。** 本节保留成因与形状，它是「端口比底层弱 → 宿主各写一份」
> 这个模式最完整的一个案例：同一段逻辑漂到了三个语言、两个抽象层。

### 曾经的样子

`FileAccess` 只有 `cleanup_sink`（丢弃**未最终化**的 sink），**没有**「删一个已落盘文件」。
于是「删收件箱条目连带删文件」这段编排在三处各写一份，且**其中一份在 TypeScript 里**：

| 端 | 编排在哪 | 怎么删 |
|---|---|---|
| 桌面 | `src-tauri/src/commands/inbox.rs` | 裸 `tokio::fs::remove_file`（绕过端口） |
| Web | `crates/web/src/node.rs` | 直调 `opfs::remove_path` |
| 移动 | `mobile/src/stores/inbox-store.ts` | `new File(localPath).delete()` |

漂出的差异：移动端在 detail 取不到时**静默跳过**，另两端报「收件箱记录不存在」——
而那恰恰是最不该静默的分支（拿不到 detail 就是不知道该删哪些文件，静默继续会让
「删了记录、文件全留下」看起来像成功）。

### 收口后的形状

- **端口补一条 `delete_finalized_file(uri)`**，`uri` 是 `finalize_sink` 返回过的那个
  （即落库的 `local_path`）。**刻意不给默认实现**——漏实现要在编译期红，而不是变成一条
  静默泄漏。补上那天四个实现方（桌面 / Web / 移动 adapter / core 的 MemoryHost）全部编译
  失败，正是想要的效果。
- **编排提到 `swarmdrop_transfer::inbox::delete_inbox_item`**，吃 `&dyn InboxStore` +
  `&dyn FileAccess`。三条不变量（先文件后记录 / 删文件失败不阻断 / 条目不存在报错）
  此前只活在三份注释里，现在有 4 条单测钉着——假 store + 假 FileAccess，不需要真的
  SQLite 或 OPFS。
- **「哪个字段是删除键」留给实现方**。上层统一递 `local_path`，桌面拿到的是文件系统绝对
  路径、移动是 `file://` 或 SAF URI、Web 是 `opfs:/` 前缀的键（实现里剥掉前缀）。
  编排一行 `cfg` 都不需要——这正是端口该吸收的那种差异。
- **桌面把 `FileAccess` 的构造提到 `setup.rs`**，`start()` 与收件箱命令取同一个 `Arc`。
  此前它建在 `start()` 里，而收件箱命令**刻意不依赖节点启动**，够不着它。

### `cleanup_sink` 那半：契约已写进 doc，三端都已实现

原本 doc 只说「丢弃一条未最终化的 sink」、默认实现是 no-op，于是「要不要真把半成品从盘上
删掉」全靠各端揣摩。现已写明「删掉部分产物是契约的一部分」。三端核对结果：

| 端 | 实现 |
|---|---|
| 桌面 | `file_source.rs` 的 `part_file.cleanup()` |
| Web | `abort()` 放锁 → `opfs::remove_path` |
| 移动 | 转发 JS callback（**此前记作「未核实」，已确认接了**） |

Web 那份的顺序有两条必须照抄：**`abort()` 而不是 `close()`**（close 会把 staging 提交
上去，正好相反），**且必须 await 到 abort 完成再删**（`createWritable()` 持文件独占锁，
只 drop 句柄要等 GC，锁没放 `remove_entry` 会撞 `NoModificationAllowedError`）。
Web 比桌面更需要它，因为 Web **没有 `.part` 中间态**——写的就是最终路径，残件是个文件名
正确、内容截断的东西。

**仍欠着**：桌面 7 天过期回收那处**绕开端口**直接 `tokio::fs::remove_file`（`database.rs`），
以及删掉 suspended 接收会话留下的 OPFS 孤儿残件。两者都该走
`delete_finalized_file`，但它们的判据是「哪些文件真没写完」，与本次的「用户主动删条目」
不是同一件事，留待单独收。

**相关文件**：`crates/host/src/ports.rs`（trait）、`crates/transfer/src/inbox.rs`（编排 + 不变量测试）、`crates/web/src/file_access.rs`、
`crates/web/src/opfs.rs`、`src-tauri/src/host/file_source.rs`、`crates/transfer/src/inbox.rs`
（`InboxItemFileEntry` 的路径字段 doc）

## 端口比底层弱时，用 crate 内 supertrait 扩展，别让宿主退回去持具体类型

`InboxStore::list_inbox_items` 按契约只给 `InboxItemSummary`（桌面的列表要的就是它）。但
Web 的收件箱本就是**全内存表**，`list()` 与 `detail()` 读同一份数据——为了拿 detail 而
「先取 N 条 summary，再逐条 `inbox_item()` 补详情」是纯粹的 1+N 浪费，还自带一个竞态
（两次调用之间条目可能已删，于是前端要 `filter(d => d !== null)`）。

诱惑是让 `WebNode` 直接持 `Arc<WebTransferStore>` 具体类型。**别这么做**——那正是本文档
反复点名的「影子副本」：宿主一旦持具体类型，端口就形同虚设，下一个功能会顺着这条路
绕过端口再写一份。

正确形态是在 **crate 内**加一条 supertrait：

```rust
// crates/web/src/store.rs
pub trait WebStore: TransferStore {
    fn list_inbox_details(&self, include_archived: bool) -> Vec<InboxItemDetail>;
}
```

组装点建**一个** `Arc<WebTransferStore>`，强转出两个视图：注入 core 的仍是
`Arc<dyn TransferStore>`（core 不该知道 Web 多一条批量读），`WebNode` 自持
`Arc<dyn WebStore>`。同一个实例，不是两份状态。

判据：**这条能力是不是所有端都该有？** 是 → 改端口（三端都要实现）；只有这一端的底层
恰好做得到 → crate 内 supertrait。`list_inbox_details` 属于后者：SQL 端做同样的事要多打
一轮 join，硬塞进端口是让桌面为 Web 的实现细节买单。

**相关文件**：`crates/web/src/store.rs`、`crates/web/src/node.rs`、`crates/web/src/inbox.rs`

## IndexedDB：批量写走一个事务，`open()` 必须做 in-flight 去重

`crates/web/src/idb.rs` 早期每次 `delete()` / `put_string()` 都新建一个 readwrite 事务。
于是「清空传输历史」= 最多 `HISTORY_CAP`(100) 个独立事务逐个 await，`prune()` 与
`reap_expired_suspended_receives` 的回写循环同一形态。

正确形态是 `write_batch`：开一个事务 → **同步**排完全部 request → 只 await 一次
`oncomplete`。注意 `fill` 闭包**必须是同步的**——IndexedDB 事务在控制权交回事件循环后
就失活，在闭包里 await 会撞 `TransactionInactiveError`。把这条约束写进签名（收同步
闭包）比写进注释可靠。

**另一个更隐蔽的坑**：`open()` 若没有 in-flight 去重，并发调用会各建一条连接。而 open
路径通常会给连接 `forget()` 一个 `onversionchange` 闭包（注释写着「每进程最多一次」）
——被 forget 的闭包**永久钉住**它所在的那条 live connection，于是多余连接永不关闭，
后续 `onupgradeneeded` 会被 blocked。修法：open 完成后再查一次缓存，晚到的
`db.close()` 并复用先到的连接。

**相关文件**：`crates/web/src/idb.rs`、`crates/web/src/store.rs`

## 相关文件

- `crates/entity/Cargo.toml:7` —— sea-orm 硬绑 runtime（第 0 步的目标）
- `Cargo.toml:21-27` —— workspace 的 sea-orm feature 定义
- `crates/core/src/database/ops.rs` —— 21 个 pub async fn，`SessionStore` 的 trait 面
- `crates/core/src/database/inbox.rs:202-258` —— 全 core 唯一跨表事务
- `crates/core/src/transfer/manager.rs:145`、`coordinator.rs:331`、`actor/receiver.rs:58` —— 3 个注入点
- `crates/core/src/transfer/flow/resume/mod.rs:479` —— 绕过 ops 层的裸 ORM 查询（动 trait 前先收编）
- `crates/core/src/host.rs` —— 既有 6 个 host trait，新 trait 的体例来源
- `crates/core/src/protocol.rs:8,148` —— `entity::TerminalReason` 上 wire（跨版本地雷）
