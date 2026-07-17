# 存储抽象（把 sea-orm 从 core 摘出去）

## 概览

2026-07-17 调研「把 SQLite 存储抽象成 trait，让 Web 端也能实现断点续传」。

方法：agent 源码级调研 + 本地编译探针。**凡标「实测」的都是真跑过 `cargo check`**，
且本文件里两条最反直觉的结论（entity 能编 wasm、SendWrapper 免改 trait）都由主线独立复核过 ——
它们分别推翻了 agent 和我自己的初始判断。

网络侧的结论在 [libp2p-wasm.md](libp2p-wasm.md)，两份不重叠。

> **当前状态：未决策，建议只做「第 0 步」。** trait 本身的价值全押在「要做 Web 端」上，
> 而 Web 端卡在更靠前的问题上（浏览器对我们的网络零可达入口，见 libp2p-wasm.md 第一节）。

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

### Web 侧后端选型

IndexedDB（`idb` / `rexie`）vs OPFS。**注意 OPFS 的 `FileSystemSyncAccessHandle` 只能在 Web Worker
里用**，而 `webrtc-websys` 在 Worker 里会 panic（见 [libp2p-wasm.md](libp2p-wasm.md)）——
两者的线程约束要一起设计，不能分开选。

文件数据本身走 `host.rs` 现有的 `FileAccess` trait（已经是 trait，OPFS 实现即可），
**只有结构化数据（session / checkpoint / 收件箱）需要新 trait**。

## 相关文件

- `crates/entity/Cargo.toml:7` —— sea-orm 硬绑 runtime（第 0 步的目标）
- `Cargo.toml:21-27` —— workspace 的 sea-orm feature 定义
- `crates/core/src/database/ops.rs` —— 21 个 pub async fn，`SessionStore` 的 trait 面
- `crates/core/src/database/inbox.rs:202-258` —— 全 core 唯一跨表事务
- `crates/core/src/transfer/manager.rs:145`、`coordinator.rs:331`、`actor/receiver.rs:58` —— 3 个注入点
- `crates/core/src/transfer/flow/resume/mod.rs:479` —— 绕过 ops 层的裸 ORM 查询（动 trait 前先收编）
- `crates/core/src/host.rs` —— 既有 6 个 host trait，新 trait 的体例来源
- `crates/core/src/protocol.rs:8,148` —— `entity::TerminalReason` 上 wire（跨版本地雷）
