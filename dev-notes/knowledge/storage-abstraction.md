# 存储抽象（把 sea-orm 从 core 摘出去）

> **状态：已落地。** 本文是 2026-07-17 的**调研快照**，记录的是切割方案的推导过程。
>
> 落地结果：
> - `crates/core` **零 sea-orm 依赖**
> - 端口 trait（`SessionStore` / `InboxStore`）在 `crates/transfer/src/store.rs`
> - SQL 实现独立成 **`crates/storage-sql`**（native-only），宿主在组装点注入
> - `crates/entity` 的 sea-orm 已 feature 解绑（Web 端可只吃类型宏）
> - Web 端走内存 store + OPFS，不依赖 storage-sql
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

> **当前状态（2026-07-27 更新）：Web 端 IndexedDB `SessionStore` 已落地（`#81`）。**
> `crates/web/src/store.rs` 的 `PersistentSessionStore` = 内存读缓存 + IndexedDB 写穿，
> 低层读写收在 `crates/web/src/idb.rs`。本文预判的两条都成立：**trait 签名一个字没改**，
> `SendWrapper` 裹 JsFuture 满足 Send；**entity 的 `Model` 直接上签名**、wasm 编译无碍。
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

3. **`InboxStore` 可以先不实现**。core 内唯一调用点是 `transfer/actor/receiver.rs:657`，
   而 `receiver.rs:656` 的注释已写明「失败只作为 DB 附加错误上报，**不回滚已完成传输**」——
   现成的降级点。

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

⇒ 又一条「Web 端先不实现 `InboxStore`」的理由。真要做，全文检索得换实现（不是换后端）。

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

## 相关文件

- `crates/entity/Cargo.toml:7` —— sea-orm 硬绑 runtime（第 0 步的目标）
- `Cargo.toml:21-27` —— workspace 的 sea-orm feature 定义
- `crates/core/src/database/ops.rs` —— 21 个 pub async fn，`SessionStore` 的 trait 面
- `crates/core/src/database/inbox.rs:202-258` —— 全 core 唯一跨表事务
- `crates/core/src/transfer/manager.rs:145`、`coordinator.rs:331`、`actor/receiver.rs:58` —— 3 个注入点
- `crates/core/src/transfer/flow/resume/mod.rs:479` —— 绕过 ops 层的裸 ORM 查询（动 trait 前先收编）
- `crates/core/src/host.rs` —— 既有 6 个 host trait，新 trait 的体例来源
- `crates/core/src/protocol.rs:8,148` —— `entity::TerminalReason` 上 wire（跨版本地雷）
