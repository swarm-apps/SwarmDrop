# inbox-store-port-completion 任务分解

> 依赖 C2 `transfer-store-port-completion` 先合（同改 `crates/transfer/src/store.rs` 与
> `crates/storage-sql/src/store.rs`，且降 impl 的体例照它）。
> 语言约定：所有新增注释与文档一律中文。
> **全局纪律（design D7）**：本 change 不写任何迁移 / 回填 / 双写 / 兼容层代码。
> 若某条实现路径只有在「要照顾存量数据」的前提下才成立，换更简洁的那条。

## 1. 端口与共享领域规则（`crates/transfer`）

- [x] 1.1 新建 `crates/transfer/src/inbox.rs`；`crates/transfer/src/lib.rs` 在
      `pub mod flow;`(:14) 与 `pub mod incoming;`(:15) **之间**插 `pub mod inbox;`
      —— 那份清单是严格字母序，`inbox` < `incoming`
- [x] 1.2 把 5 个 DTO 从 `crates/storage-sql/src/inbox.rs:17-90` 整体迁入
      `crates/transfer/src/inbox.rs`：`InboxItemSummary` / `InboxItemFileEntry` /
      `InboxItemDetail` / `InboxSearchHit` / `InboxHitFile`。
      **字段、`#[serde(rename_all = "camelCase")]`、`#[serde(flatten)]`、
      `#[cfg_attr(feature = "specta", derive(specta::Type))]` 逐条原样保留**——
      任何字段改动都会让 9.x 的「桌面 bindings diff 应为空」这条检查失效
- [x] 1.3 定义中立文件视图 `InboxFileFacts<'a> { name, relative_path, checksum, size }`
      （design D2）——共享规则一律吃它，**不吃 `entity::*::ModelEx`**
- [x] 1.4 上提 `inbox_title`（原 `crates/storage-sql/src/inbox.rs:501`），签名改
      `&[InboxFileFacts<'_>]`，三分支行为不变
- [x] 1.5 上提 `inbox_content_hash`（原 `:509`），blake3 累加顺序
      `relative_path ‖ 0x00 ‖ checksum ‖ size.to_le_bytes()` **逐字节不变**
- [x] 1.6 上提 FTS 聚合文本拼法为 `inbox_files_text`（原 `:195-201` 的 inline
      `format!("{} {}", file.name, file.relative_path)` + `join(" ")`）
- [x] 1.7 上提 `source_kind_for_origin` → `inbox_source_kind`（原 `:522`），
      内部仍走 `crate::protocol::TransferOrigin::from_db_string`
- [x] 1.8 上提 `make_snippet` + `snippet_window`（原 `:564` / `:578`）为
      `inbox_snippet(query, title, source_name, files)`；`CTX = 16` 与「按字符切窗口、
      UTF-8 安全、首尾加 `…`」的行为不变
- [x] 1.9 新增 `inbox_matches(query, title, source_name, files_text) -> bool`：
      大小写不敏感子串，**这是检索命中判据的规范定义**（design D2），
      文档注释写明 SQL 侧的 `LIKE` 必须复刻它
- [x] 1.10 `crates/transfer/src/inbox.rs` 内嵌单测：标题三分支（0 / 1 / N 文件）、
      `inbox_content_hash` 的**已知向量**（钉死十六进制串，防两端漂移）、
      `inbox_snippet` 的 CJK 与首尾边界、`inbox_matches` 的大小写与 2 字中文词（"合同"）
- [x] 1.11 `crates/transfer/src/store.rs:113` 的 `InboxStore` 从 1 个方法补到 **10 个**：
      `ensure_inbox_item_for_completed_receive_session` /
      `repair_missing_inbox_items_for_completed_receives` / `list_inbox_items` /
      `search_inbox` / `get_inbox_item_detail` / `get_inbox_item_by_transfer_session_id` /
      `mark_inbox_item_opened` / `archive_inbox_item` / `delete_inbox_item_record` /
      `mark_inbox_item_file_missing`。名字与 `crates/storage-sql/src/inbox.rs` 的自由函数
      **逐字对齐**（不改名，保 grep 可达）
- [x] 1.12 `ensure_inbox_item_for_completed_receive_session` 返回值
      `AppResult<()>` → `AppResult<Option<InboxItemDetail>>`（design D3），
      并改写它现有那段「调用方 receiver 不消费 detail」的注释
- [x] 1.13 `mark_inbox_item_file_missing` 签名加 `item_id: Uuid`（design D4），
      文档写明归属校验是**端口的义务**而非各宿主自理
- [x] 1.14 `crates/transfer/src/actor/receiver.rs:709` 的
      `ensure_inbox_item_after_completion` 跟随 1.12 的返回值变化——仍然只在 `Err` 分支
      记日志 + 发 `TransferDbError`，`Ok(_)` 不消费
- [x] 1.15 `crates/transfer/src/store.rs` 模块头注释（`:1-9`）更新：端口不再只有「运行时写路径」，
      收件箱的管理类语义（列表 / 检索 / 归档 / 软删）也在里面
- [x] 1.16 检查 `crates/transfer` 全 crate 无新增 `sea_orm::` 与 `ModelEx` 引用
      （`rg 'sea_orm|ModelEx' crates/transfer/src` 应只命中 `store.rs:195 / 212 / 225`
      这三处**既有**的 `transfer_file::ModelEx` 转换——本 change 不新增第四处）

## 2. SQL 实现降 impl（`crates/storage-sql`）

- [x] 2.1 `crates/storage-sql/src/inbox.rs` 删掉 5 个 DTO 定义（`:17-90`），
      改 `use swarmdrop_transfer::inbox::{...}`
- [x] 2.2 `From<entity::inbox_item_file::ModelEx>`(`:92`) 与
      `From<&entity::inbox_item::ModelEx>`(`:107`) 两个转换**留在本文件**——
      它们吃 `ModelEx`，是 SQL 实现细节
- [x] 2.3 删掉本文件的 `inbox_title`(`:501`) / `inbox_content_hash`(`:509`) /
      `source_kind_for_origin`(`:522`) / `make_snippet`(`:564`) / `snippet_window`(`:578`)，
      全部改调 `swarmdrop_transfer::inbox::*`；`:195-201` 的 inline 聚合文本改调 `inbox_files_text`
- [x] 2.4 `escape_like`(`:530`) 保持私有、留在本文件（SQL `LIKE` 转义，非领域规则）
- [x] 2.5 `:334` `search_inbox` 的 raw SQL 上方补注释：这段 `LIKE ... ESCAPE '\'` 是
      `swarmdrop_transfer::inbox::inbox_matches` 的 SQL 复刻，两者语义必须同义
- [x] 2.6 10 个 `pub async fn` 降为 private（`:151` / `:266` / `:302` / `:334` /
      `:406` / `:426` / `:443` / `:453` / `:467` / `:477`）
- [x] 2.7 `crates/storage-sql/src/store.rs:153` 的 `impl InboxStore for SqlSessionStore`
      补齐 10 个方法，逐条委托本 crate 的私有函数（体例照同文件 `:32` 起的 `SessionStore` impl）
- [x] 2.8 `ensure_inbox_item_for_completed_receive_session` 去掉 `:161` 那句 `.map(|_| ())`
      与 `:158` 那条「端口不透出」的注释——端口现在要 `Option<InboxItemDetail>`
- [x] 2.9 `mark_inbox_item_file_missing` 实现里加归属校验：按 `item_id` 取条目、
      断言 `file_id` 属于它，否则 `AppError::Transfer`。逻辑搬自
      `mobile/packages/swarmdrop-core/rust/mobile-core/src/inbox.rs:320-328`（那段随之删除，见 5.4）
- [x] 2.10 `crates/storage-sql/src/inbox.rs` 的 13 条测试（`:637` 的 `#[test]` +
      `:713` 起的 12 条 `#[tokio::test]`）改成经 `SqlSessionStore` 调端口方法而非直调私有函数——
      测试是「端口行为」的证据，不是实现细节的证据。
      `clear_history_should_keep_inbox_records`(`:797`) 这条**必须保留**，
      它钉着「清空活动不动收件箱」这条不变量
- [x] 2.11 `crates/core/tests/e2e_transfer.rs:537` 的
      `swarmdrop_storage_sql::inbox::ensure_inbox_item_for_completed_receive_session(...)`
      直调改成经 `SqlSessionStore`（该文件 `:43` 已 import 了 `SqlSessionStore`）

## 3. 宿主组装点：端口实例建一次、注入与自持同一个 `Arc`（design D5）

- [x] 3.1 `src-tauri/src/database.rs` 加
      `pub type TransferStoreState = std::sync::Arc<dyn swarmdrop_transfer::store::TransferStore>;`
      （避免命令签名里出现裸 `Arc<dyn ...>` 难读）
- [x] 3.2 `src-tauri/src/setup.rs` 在 `app.manage(db)` 之后立刻建一份 `SqlSessionStore`
      并 `app.manage(store as TransferStoreState)`
- [x] 3.3 `src-tauri/src/database.rs:47` 的 `cleanup_stale_sessions` 改为**接收** store 参数，
      不再在 `:52` 内部新建 `SqlSessionStore`
- [x] 3.4 `src-tauri/src/commands/lifecycle.rs:84` 的工厂闭包改为捕获 3.2 建的那一份
      （从 `app.try_state::<TransferStoreState>()` 取），删掉那处 `SqlSessionStore::new`
- [x] 3.5 移动端同样收敛：`mobile-core/src/network.rs:235` 与 `mobile-core/src/history.rs:211`
      两处 `SqlSessionStore::new` 合成一处，由 `MobileCore` 持有（与 `ensure_db()` 同一生命周期），
      网络启动时注入 manager
- [x] 3.6 两个组装点各补一句注释：**注入 `TransferManager` 的与宿主自持的是同一个 `Arc`**
      （不是两个包装同一条连接的实例）——这是 D5 的全部纪律所在

## 4. 桌面调用点切换（`src-tauri`）

- [x] 4.1 `src-tauri/src/commands/inbox.rs`：`list_inbox_items`(`:18`) 的
      `db: State<'_, DatabaseConnection>` 换成 `store: State<'_, TransferStoreState>`
- [x] 4.2 同文件 `get_inbox_item_detail`(`:27`) / `get_inbox_item_by_transfer_session_id`(`:36`) /
      `search_inbox`(`:48`) / `repair_missing_inbox_items`(`:63`) 四条同样切换
- [x] 4.3 同文件 `archive_inbox_item`(`:131`) / `delete_inbox_item`(`:141`) 切换；
      `delete_inbox_item` 里 `:153` 的 `mark_inbox_item_file_missing` 补传 `item_id`
- [x] 4.4 同文件 `open_inbox_item`(`:71`) / `show_inbox_item_in_folder`(`:87`) /
      `export_inbox_item`(`:107`) 切换——**平台动作（`tauri_plugin_opener` / `tokio::fs::copy`）
      留在命令里**，只有查询与标记走端口
- [x] 4.5 同文件三个私有 helper `load_inbox_detail`(`:163`) / `ensure_path_exists`(`:194`) /
      `ensure_file_exists`(`:214`) 的 `db` 参数换成 store；后两者调
      `mark_inbox_item_file_missing` 处（`:204` / `:206` / `:222`）补 `item_id`
- [x] 4.6 `src-tauri/src/mcp/tools.rs:379` `search_inbox` 的
      `try_state::<DatabaseConnection>()` 换成 `try_state::<TransferStoreState>()`
- [x] 4.7 同文件 `:411` `get_inbox_file` 切换
- [x] 4.8 同文件 `:582` `list_inbox` 切换
- [x] 4.9 同文件 `:612` `get_inbox_item` 切换
- [x] 4.10 同文件 `:765` `archive_inbox_item` 切换
- [x] 4.11 同文件 `:793` `export_inbox_item` 切换（它 `:801` 转调
      `crate::commands::export_inbox_item`，参数类型跟着 4.4 变）
- [x] 4.12 **不动** `tools.rs:453 / :480 / :673` 三处 `try_state::<DatabaseConnection>()`
      —— 它们是 `list_transfers` / `get_transfer_status` / `update_session_origin`，
      属于 C2 的边界
- [x] 4.13 `src-tauri/src/database.rs:7` 的 `pub use swarmdrop_storage_sql::{inbox, ops};`
      收窄——`inbox` 模块降 private 后已无 pub 项可转发，那半边删掉；
      模块头注释（`:1-5`）同步改写

## 5. 移动调用点切换（`mobile-core`）

- [x] 5.1 `mobile/packages/swarmdrop-core/rust/mobile-core/src/inbox.rs:8` 的
      `use swarmdrop_storage_sql::inbox as inbox_ops;` 换成 `swarmdrop_transfer::inbox`（DTO）
      + 经 store 调方法
- [x] 5.2 同文件 9 个 uniffi 方法（`:252` / `:263` / `:275` / `:287` / `:295` / `:303` /
      `:311` / `:334` / `:344`）从 `inbox_ops::*(&db, ...)` 改成经 3.5 持有的 store 调用
- [x] 5.3 同文件 5 个 `From<inbox_ops::*>` impl（`:73` / `:127` / `:160` / `:183` / `:211`）
      来源类型换成 `swarmdrop_transfer::inbox::*`；**穷尽解构的 drift guard 保持**
- [x] 5.4 同文件 `mark_inbox_file_missing`(`:311`) 里 `:320-328` 的归属校验删除
      （已由 2.9 上提到端口），改为直接把 `item_id` 传下去
- [x] 5.5 确认 uniffi 导出**签名形状不变**（Record 字段、方法签名逐条比对），
      故无需重跑 `pnpm --filter react-native-swarmdrop-core build:ios`

## 6. Web：建真收件箱表（`crates/web`）

- [x] 6.1 `crates/web/src/idb.rs` 加 `pub const INBOX_STORE: &str = "inbox";`（key = item uuid）
- [x] 6.2 同文件 `DB_VERSION`(`:29`) 3 → 4
- [x] 6.3 同文件 `install_upgrade_handler` 的
      `for name in [KV_STORE, SESSION_STORE, INVITE_STORE]`(`:142`) 加 `INBOX_STORE`
      ——**漏这条新 store 建不出来，且只在运行时报错**
- [x] 6.4 同文件 `:27` 的版本沿革注释补 v4；`:3-7` 的「两个 object store」表述改成四个
- [x] 6.5 新建 `crates/web/src/inbox.rs`：自包含的 `WebInboxTable`
      （自有 `Mutex<HashMap<Uuid, StoredInboxItem>>` + 全部 IndexedDB 读写）；
      `crates/web/src/lib.rs` 加 `#[cfg(wasm_browser)] mod inbox;`
- [x] 6.6 同文件用 serde **remote derive** 声明持久化形态
      （`entity::inbox_item::Model` + `entity::inbox_item_file::Model`），
      体例照 `crates/web/src/store.rs:544-604` 的 `SessionRowDef` / `FileRowDef`
- [x] 6.7 `WebInboxTable::load()` 全量 `idb::get_all(INBOX_STORE)`；
      单条记录解析失败只 `warn` 跳过（照 `store.rs:94-104`）；读不到库时退化成纯内存
      （照 `store.rs:81-87`）
- [x] 6.8 实现 9 个条目级方法。`ensure_from_session(&session, &files)` **接收会话行 + 文件行
      作为参数**，不反向去读 `WebTransferStore` 的 sessions map（design D10 的依赖方向）
- [x] 6.9 `repair_missing_inbox_items_for_completed_receives` 实现在 **`WebTransferStore`** 上
      （它同时握着 sessions map 与 inbox 表），循环里调 `ensure_*`，复用同一条构造路径。
      **注意它只是端口方法，不在启动路径上被调用**（见 7.1）
- [x] 6.10 条目构造全程调 `swarmdrop_transfer::inbox` 的共享规则：
      `inbox_title` / `inbox_content_hash` / `inbox_source_kind` / `inbox_files_text`；
      `root_path` 用 `crates/web/src/store.rs:665` 已有的 `content_root_of` 内联版
- [x] 6.11 `search_inbox` 用 `inbox_matches` + `inbox_snippet`：过滤 `deleted_at` 非空、
      按 `include_archived` 过滤 `archived_at`、按 `received_at` 倒序、截断 `limit`（design D8）
- [x] 6.12 `InboxItemFileEntry.id` 用**条目内序号**（0..n，写入时定、之后不变）；
      `transfer_file_id` 填对应 `entity::transfer_file::Model.file_id`（design D9）
- [x] 6.13 `crates/web/src/store.rs` 的 `PersistentSessionStore` 改名 `WebTransferStore`
      （`:72` 定义 + `:79` / `:267` / `:525` 三个 impl 头），新增 `inbox: WebInboxTable` 字段；
      `:524-534` 的 no-op `impl InboxStore` 换成逐条委托 `self.inbox.*`
- [x] 6.14 同文件模块注释 `:31-35`「不落库的两样东西 → `InboxStore`」那条**删掉**，
      改写成「收件箱是独立的 `inbox` store，不参与 `HISTORY_CAP` 淘汰」；
      `:31` 的小标题从「两样」改「一样」
- [x] 6.15 同文件 `prune()`(`:173`) 的文档注释补一句：**只淘汰 sessions，收件箱条目不在其列**
      ——这是真表相对投影方案的实质差异，值得在代码里写死
- [x] 6.16 `crates/web/src/node.rs` 的 `use crate::store::PersistentSessionStore;`(`:41`)、
      字段声明(`:139`)、`::load()`(`:175`) 跟随改名；`:182` / `:219` / `:250` 的
      `session_store` 变量名可保留（它确实还是会话 store 的持有者）
- [x] 6.17 **补记（本清单原本漏了 Web 侧测试）**：给 `crates/web` 的收件箱补 11 条
      `wasm_bindgen_test`（`inbox.rs` 8 条 + `store.rs` 3 条），钉住这几条不变量——
      ① `prune()` 淘汰会话但**不淘汰收件箱条目**（10.16 的机器版，会话被淘汰后
      条目与文件行仍在、`transfer` 转 `None`），以及用户手点的 `clear_all_history`
      同样不动收件箱（与 SQL 侧 `clear_history_should_keep_inbox_records` 同形）；
      ② 条目跨「重启」存活（写穿 IndexedDB →
      新实例 `load()` → 整条详情的 JSON 逐字段一致，钉 remote derive 往返）；
      ③ `inbox_content_hash` 与 `swarmdrop_transfer::inbox` 的已知向量**同输入同输出**
      （跨端去重判据不漂移）；④ `search` 的命中判据（只出现在 `relative_path` 里的词也命中）
      与软删 / `include_archived` / `received_at` 倒序 / `limit` 截断；⑤ `list` 的可见性与倒序
      （与 search 各有一份实现，不能只测一边）；⑥ `ensure_from_session` 幂等；
      ⑦ 非「Receive + Terminal + Completed」一律 `Ok(None)`；⑧ 缺 `save_path` / 缺 `local_path`
      显式报错且不留半条条目；⑨ `mark_file_missing` 的归属校验（Web 侧 `file_id` 只是条目内序号）
      + 条目级 `missing` 聚合；⑩ `repair_*` 只补缺的、软删过的不复活。
      跑法（`wasm-pack test` 在本机因 ChromeDriver 版本不匹配跑不通，绕开它直驱 cargo）：
      `CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=<wasm-bindgen-test-runner> CHROMEDRIVER=<chromedriver> WASM_BINDGEN_TEST_ONLY_WEB=1 cargo test --target wasm32-unknown-unknown`

## 7. Web：显式**不做**迁移与回填（design D7）

- [x] 7.1 确认 `WebTransferStore::load()` **没有**在任何路径上调用
      `repair_missing_inbox_items_for_completed_receives` —— 老库里的已完成接收会话
      不补条目，收件箱从空开始
- [x] 7.2 确认没有任何「读旧格式 → 写新格式」的分支、没有双写、没有 `if db_version < 4` 之类的
      兼容代码。`onupgradeneeded` 只做「建缺失的 store」这一件事（`idb.rs:130-151` 的现有语义
      本来就是这样，不要为本 change 加特例）
- [x] 7.3 在 `crates/web/src/inbox.rs` 模块头写明这条决策与理由（无存量用户 + 简洁优先），
      并指向本 change 的 design D7 —— 免得后人看到「空收件箱 + 满历史」以为是 bug

## 8. Web：wasm 导出与前端换数据源

- [x] 8.1 `crates/web/src/types.rs` 按 `pub use swarmdrop_host::device::Device;`(`:11`) 的体例
      re-export 5 个收件箱 DTO
- [x] 8.2 `crates/web/src/lib.rs` 的 `pub use types::{...}` 清单同步
- [x] 8.3 `crates/web/tests/specta_export.rs:56-64` 的 `Types::default().register::<...>()`
      链补上收件箱类型；顺手把 `:2` 那句「导成 `static/types/bindings.ts`」修正为
      `bindings/bindings.ts`（与 `:72` 的实际路径一致）
- [x] 8.4 `crates/web/src/node.rs` 的 `#[wasm_bindgen] extern "C"` 块（`:57-86`）加
      `InboxItemSummary[]` / `InboxSearchHit[]` 两个 `typescript_type` 包装
- [x] 8.5 `crates/web/src/node.rs` 新增 7 个导出：`inbox_items` / `inbox_item` /
      `inbox_item_by_session` / `search_inbox` / `mark_inbox_item_opened` /
      `archive_inbox_item` / `delete_inbox_item`。
      **不导出 `mark_inbox_item_file_missing` 与 `repair_*`**（design D13 写明理由）
- [x] 8.6 `cargo test -p swarmdrop-web --features specta --test specta_export` 再生
      `crates/web/bindings/bindings.ts`（**会变**，新增 5 个类型）
- [x] 8.7 `docs/` 下 `pnpm build:wasm` 重新生成 `docs/packages/swarmdrop-web/`
- [x] 8.8 `docs/app/app/_lib/view-types.ts` 的 `export type { ... } from "swarmdrop-web"`
      补上收件箱类型
- [x] 8.9 `docs/app/app/_lib/store.ts` 加 `inboxItems` 域 + 一个 `setInboxItems` action；
      **排序在写入时做完**，selector 只返回稳定引用
      （`_lib/create-store.ts` 是自研 store，`pnpm check:zustand-access` 不覆盖 `docs/`，
      这里没有机器兜底）
- [x] 8.10 `docs/app/app/_components/receive-panel.tsx` 的 `InboxPanel`(`:113`)
      从 `Object.values(projections).filter(...)`(`:119-125`) 换成读 `inboxItems`
- [x] 8.11 挂载时拉一次 + `transferCompleted`（`direction=receive`）事件到达时重拉（design D13）；
      拉取入口挂在 `InboxPanel` 内即可，**不要**下放到 `WebNodeBootstrap`
      （那是运行时单例的位置，不是数据拉取的位置）
- [x] 8.12 `docs/app/app/inbox/page.tsx:8-10` 那条「收件箱是结果 / 传输页是过程」的分工注释
      改写：分工不变，但依据从「同一份 projection 的两种过滤」变成「两张各自的表」

## 9. 文档与知识库

- [x] 9.1 `dev-notes/knowledge/storage-abstraction.md`：Web 端一节从
      「IndexedDB 写穿的 `SessionStore`」更新为「`SessionStore` + 独立 `InboxStore` 表」，
      并记下 D2 的「领域规则住在 transfer、各存储实现调它」这条体例
- [x] 9.2 `dev-notes/knowledge/web-app-frontend.md`：补 IndexedDB 版本升级的三处同改
      （store 常量 / `DB_VERSION` / `onupgradeneeded` 清单）这条坑
- [x] 9.3 `CLAUDE.md` 的「Web 端（wasm）」一段：「持久化是『内存读缓存 + IndexedDB 写穿』的
      `SessionStore`」改成含收件箱表；Key File Locations 里 `crates/web/`
      （`store.rs` 是…）那一行同步
- [x] 9.4 `cargo test export_ts_bindings` 再生 `src/lib/bindings.ts`，
      **确认 diff 为空**（design D14；非空说明 1.2 的字段搬迁出了偏差）

## 10. 门禁与验证

- [x] 10.1 `cargo fmt --all`
- [x] 10.2 `cargo check --workspace --all-targets`
- [x] 10.3 `cargo test --workspace`（重点看 `crates/storage-sql` 的 13 条 inbox 测试与
      `crates/core/tests/e2e_transfer.rs` 里那条收件箱断言）
- [x] 10.4 `cargo clippy --workspace`
- [x] 10.5 `./scripts/check-wasm.sh`
- [x] 10.6 `./scripts/check-wasm.sh --clippy`
- [x] 10.7 `src/lib/bindings.ts` diff 为空的确认（9.4 的结果；非空要回查 1.2）
- [x] 10.8 `pnpm test`（仓库根 vitest）
- [x] 10.9 `pnpm exec tsc --noEmit`（仓库根）
- [x] 10.10 `docs/` 下 `pnpm build`（静态导出三限制：无动态段 / `next/link` /
      `useSearchParams` 套 Suspense——本 change 不新增路由，但 `pnpm build` 仍是唯一能抓到
      CSR bailout 的门）
- [x] 10.11 `mobile/` 下 `pnpm typecheck`
- [x] 10.12 `pnpm check:zustand-access` —— **本 change 不碰仓库根 `src/`**，
      跑一次确认无回归即可（`docs/` 不在它的扫描范围，8.9 的纪律靠人工）
- [ ] 10.13 **人工回归 · 桌面**：在**未启动节点**的情况下打开收件箱页，
      列表 / 详情 / 打开文件三条仍可用（design D5 选型的全部理由）
- [ ] 10.14 **人工回归 · 桌面**：清空传输历史后收件箱条目仍在
- [ ] 10.15 **人工回归 · 桌面**：把某条已接收文件从磁盘删掉再点「打开」，
      应报「本地文件不存在」且该文件被标记 `missing`（4.5 补的 `item_id` 传参要验到）
- [ ] 10.16 **人工回归 · Web**：接收一次 → 刷新页面 → 收件箱条目仍在；
      再制造超过 `HISTORY_CAP`（100）条终态会话使淘汰触发后，早期收件箱条目**仍在**
      （这是真表相对投影方案的实质差异，必须实测到）
- [ ] 10.17 **人工回归 · Web**：带旧数据（v3 库）的浏览器打开一次，验证
      `onupgradeneeded` 建出 `inbox` store、读写正常、**且收件箱按 D7 从空开始**
      （看到空列表是预期结果，不是 bug）
- [ ] 10.18 **人工回归 · 移动**：收件箱列表 / 详情 / 打开 / 分享 / 归档 / 删除各走一次
      （5.4 删掉了桥接层的归属校验，要确认端口侧那份生效——传一个不属于该条目的 file_id
      应报错）
