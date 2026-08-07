# transfer-store-port-completion 任务分解

> 闭合 R1（端口覆盖不全）+ R2（端口无出口），解锁 issue #104。
> 阶段顺序即依赖顺序：1 定契约 → 2 落 SQL 实现 → 3/4/5 三端并行切换 → 6 测试 → 7 门禁。
> **每个 Rust 阶段结束都跑一次 `./scripts/check-wasm.sh`**（design 末节：DTO 纪律破功只在
> wasm target 上暴露，越晚发现改动面越大）。

## 1. 端口契约（`crates/transfer`）

- [x] 1.1 `crates/transfer/src/store.rs`：`SessionStore` 新增
      `async fn list_transfer_projections(&self) -> AppResult<Vec<TransferProjection>>`，
      doc 注释写明**按 `started_at` 倒序**是契约的一部分（design D3），
      并注明「前端各面板按自己的维度重排是预期行为，端口保证的是确定性」
- [x] 1.2 同文件新增 `async fn delete_session(&self, session_id: Uuid) -> AppResult<()>`，
      doc 写明级联到该会话的文件行、**不删收件箱条目**（`inbox_items.transfer_session_id`
      是 `ON DELETE SET NULL`）、**不删已落盘文件**（design D5）
- [x] 1.3 同文件新增 `async fn clear_all_history(&self) -> AppResult<()>`，
      doc 写明**只删 `phase = Terminal` 的会话**，非终态保留（design D6）
- [x] 1.4 同文件新增 `async fn get_session_source_paths(&self, session_id: Uuid) -> AppResult<Vec<String>>`
- [x] 1.5 同文件新增
      `async fn reap_expired_suspended_receives(&self, retention_secs: u64) -> AppResult<Vec<ExpiredReceiverActor>>`，
      doc 写明命中判据是 `phase = Suspended && recoverable && direction = Receive && updated_at < now - retention`，
      **且调用点必须排在 `cleanup_recoverable_sessions()` 之后**（design D7）
- [x] 1.6 同文件新增
      `async fn update_session_origin(&self, session_id: Uuid, origin: TransferOrigin) -> AppResult<()>`
- [x] 1.7 更新 `SessionStore` 的模块级/trait 级 doc：说明端口现在覆盖**运行时写路径 + 历史管理**
      两类，不再有「一半在 trait 外」的状态
- [x] 1.8 `crates/transfer/src/store.rs:212` `content_root_of` 签名改为
      `(files: &[entity::transfer_file::Model], save_path: Option<&CoreSaveLocation>)`
      （design D2）；同文件 :231 的 `From<ModelEx> for TransferProjection` 跟着调整
- [x] 1.9 同文件新增 `pub fn is_deletable(session: &entity::transfer_session::Model) -> bool`
      （`Terminal | Suspended`），作为 D4 守卫的单一判据
- [x] 1.10 `crates/transfer/src/manager.rs`：在 `file_access()`（:261）后加
      `pub fn store(&self) -> &Arc<dyn TransferStore>`，doc 注释写明
      「纯读与无生命周期语义的写经此；带生命周期语义的删除走 `delete_session()`」
- [x] 1.11 同文件新增
      `pub async fn delete_session(&self, session_id: Uuid) -> AppResult<()>`：
      `store().find_session()` → `is_deletable()` 判定 → 拒绝时返回带明确原因的
      `AppError::Transfer`（文案指向「请先取消」）→ 通过则委托 `store().delete_session()`

## 2. SQL 实现（`crates/storage-sql`）

- [x] 2.1 `crates/storage-sql/src/store.rs`：`impl SessionStore for SqlSessionStore` 补上
      1.1–1.6 六个方法（保持现有「方法体委托 `ops`」的形态先跑通编译）
- [x] 2.2 把 `ops.rs:355 get_transfer_projections` 的函数体搬进 `store.rs` 的
      `list_transfer_projections`，`ops.rs` 里的自由函数删除；确认
      `load_sessions_with_files`（:451）的 `order_by_desc(StartedAt)`（:456）保留
- [x] 2.3 同样搬 `ops.rs:461 delete_session`（`cascade_delete` 行为不变）
- [x] 2.4 搬 `ops.rs:473 clear_all_history`，**并按 D6 加终态过滤**：
      文件行改为按「属于终态会话」的子查询删，会话行加
      `.filter(Column::Phase.eq(TransferPhase::Terminal))`
- [x] 2.5 搬 `ops.rs:492 get_session_source_paths`（保留 `select_only` + SQL 侧
      `is_not_null` 下推，不要退化成全列物化）
- [x] 2.6 搬 `ops.rs:513 reap_expired_suspended_receives`
- [x] 2.7 搬 `ops.rs:128 update_session_origin`
- [x] 2.8 `ops.rs:203 update_sender_file_progress` 降 `pub` → private（唯一调用者是同文件
      :254 的 `save_sender_file_progress`）
- [x] 2.9 **删除 `ops.rs:278 mark_session_completed` 与 `ops.rs:312 mark_session_paused`**
      （design D8），连同 :286-288 那段「过渡期桥接」注释
- [x] 2.10 修 `ops.rs:648` / :650 的自身测试：改为直构 `ActiveModel` 落库或经
      `apply_transition` 达成目标 phase
- [x] 2.11 修 `crates/storage-sql/src/inbox.rs` 的 6 处
      （:606 import、:718、:748、:769、:801、:847）—— 同上
- [x] 2.12 修 `crates/core/tests/e2e_transfer.rs:311`（`mark_session_paused`）与
      :1267（`ops::reap_expired_suspended_receives` → 改调 store 方法）
- [x] 2.13 `crates/storage-sql/src/inbox.rs:188` 的 `content_root_of` 调用按 1.8 的新签名调整
- [x] 2.14 `ops.rs` 顶部 / `store.rs` 顶部注释更新：`store.rs` 不再是「委托 ops 的薄壳」，
      历史管理六类的实现本体就在这里

## 3. 桌面切换（`src-tauri` + 根 `src/`）

- [x] 3.1 `src-tauri/src/commands/transfer.rs:195 get_transfer_projections`：
      去掉 `db: State<'_, DatabaseConnection>`，改走 `get_transfer(&net).await?.store().list_transfer_projections()`
- [x] 3.2 同文件 :203 `delete_transfer_session` → `get_transfer(&net).await?.delete_session(session_id)`
      （走 D4 的域方法，不是 `store().delete_session()`）
- [x] 3.3 同文件 :212 `clear_transfer_history` → `store().clear_all_history()`
- [x] 3.4 同文件 :221 `get_transfer_source_paths` → `store().get_session_source_paths()`
- [x] 3.5 同文件 :259 `resume_transfer`：摘掉 `db` State，:266 的
      `entity::TransferSession::find_by_id(...).one(db)` 换成 `store().find_session()`
      （审计原稿漏列的第五条）
- [x] 3.6 `src-tauri/src/mcp/tools.rs:456`（`list_transfers`）改走 manager 的 store；
      注意保留 :461 那句 `sort_by_key(Reverse(updated_at))`——端口给的是 started_at 序，
      MCP 要的是 updated_at 序，两者并存是对的
- [x] 3.7 `tools.rs:486`（`get_transfer_status`）→ `store().get_transfer_projection()`
- [x] 3.8 `tools.rs:674`（MCP 代收后打 origin 标记）→ `store().update_session_origin()`，
      连同 :673 的 `try_state::<DatabaseConnection>()` 一起删掉
- [x] 3.9 `src-tauri/src/database.rs:47 cleanup_stale_sessions`：:51 已经构造了
      `SqlSessionStore`，把它提出来复用；:60 的 `ops::reap_expired_suspended_receives(db, …)`
      改为 store 方法调用。`db` 参数保留（还要建 store）
- [x] 3.10 `src-tauri/src/database.rs:207` 的测试改写（D8：`mark_session_paused` 已删）
- [x] 3.11 `src-tauri/src/database.rs:7` 的 `pub use swarmdrop_storage_sql::{inbox, ops}`：
      确认 `ops` 仍被谁需要（`TransferProjection` 的路径别名 / `now_ms`），
      不需要的 re-export 一并收掉
- [x] 3.12 重新导出 bindings：`cargo test export_ts_bindings`。
      **预期 `src/lib/bindings.ts` 零 diff**（State 参数不进 IPC 签名，返回类型未变）——
      若有 diff 说明哪里签名动了，要先弄清楚
- [x] 3.13 `src/routes/_app/transfer/-session-row.tsx:298` 的 `!isActive` 门保持不变，
      但确认文案（:323-330）补一句「进行中的传输请先取消」的兜底提示，与后端新守卫对齐
- [x] 3.14 `src/routes/_app/transfer/index.lazy.tsx:161-163` 清空后的 `loadProjections()`
      保持不变（D6 后进行中会话会被刷回来，UI 无需改），但清空确认文案改成
      「清空已结束的传输记录」，别再暗示会连进行中一起清
- [x] 3.15 **顺带补齐三端一致**（C1 的遗留）：`src-tauri/src/commands/transfer.rs` 的
      `pause_transfer` 仍是「先试 send 失败再试 receive」的试错实现，且把两条错误串拼成一句。
      移动端已在 C1 按方向拆成四条导出，桌面这一侧还欠着。拆成 `pause_send` / `pause_receive`
      两条命令，前端按 `projection.direction` 分派（判据用现成的，不要另写）。
      `cancel_transfer` 若同款则一并拆。**不留兼容包装** —— 留着等于把试错逻辑留在代码里给人照抄

## 4. 移动端切换（`mobile/`）

- [x] 4.1 `mobile-core/src/history.rs:237` `get_transfer_projections` → 经
      `self.transfer_manager_arc().await?.store().list_transfer_projections()`
- [x] 4.2 同文件 :249 `get_transfer_projection` → `store().get_transfer_projection()`
- [x] 4.3 同文件 :258 `delete_transfer_record` → `transfer_manager_arc().delete_session()`（D4 域方法）
- [x] 4.4 同文件 :265 `clear_transfer_activity` → `store().clear_all_history()`
- [x] 4.5 同文件 :218 `reconcile_stale_sessions` 里的
      `ops::reap_expired_suspended_receives` → 用 :211-212 已构造的 `SqlSessionStore`
      （提出来复用，别再造第二个）调 store 方法
- [x] 4.6 同文件 :279 `resume_transfer` 里的 `ops::get_transfer_projection` 同款替换
- [x] 4.7 **新增 uniffi 导出** `pub async fn get_transfer_source_paths(&self, session_id: String) -> FfiResult<Vec<String>>`
      （`mobile-core/src/history.rs`），委托 `store().get_session_source_paths()`
- [x] 4.8 `pnpm --filter react-native-swarmdrop-core build:ios` 重建桥接，确认
      `mobile/packages/swarmdrop-core/src/generated` 出现新方法
- [x] 4.9 `mobile/src/stores/transfer-store.ts` 加 `getSourcePaths(sessionId)` 动作
      （与 :201 `deleteTransferRecord` 同形），错误按现有 `errorMessage` 走
- [x] 4.10 `mobile/src/app/transfer/[sessionId].tsx:174` 的「重新发送」升级：
      先取 `getSourcePaths`，非空则塞 `share-store` + 跳发送流；
      **空或路径失效时回退到现有的「预选设备后重新挑文件」**，并把 :174-175 那段
      「核心里没有 resend API」的注释改写。Android 的 SAF content URI 可能已失效，
      不做「假装能一键重发」（这正是原注释的顾虑，不要把它丢掉）
- [x] 4.11 i18n：新增串补 `mobile/src/locales/{zh-Hans,zh-Hant,en}/messages.po`

## 5. Web 端（`crates/web` + `docs/app/app`）

- [x] 5.1 `crates/web/src/store.rs`：`impl SessionStore for PersistentSessionStore` 补 1.1–1.6
      六个方法
- [x] 5.2 `list_transfer_projections`：由现有 :129 `all_projections()` 改造而来，
      **补 `sort_unstable_by_key(|p| Reverse(p.started_at))`**；:126-128 那段
      「不排序」的注释改写成指向本 change 的 D3
- [x] 5.3 `delete_session`：内存 map 删 + `idb::delete(SESSION_STORE, key)`
      （:163 `forget()` 已经是这套，直接复用）
- [x] 5.4 `clear_all_history`：只删 `phase == Terminal` 的（D6），复用 `forget()` 逐条删
- [x] 5.5 `get_session_source_paths`：从内存 map 读 `files[].source_path`，过滤 `None`
- [x] 5.6 `reap_expired_suspended_receives`：把 :243 `reap()` 的字段赋值搬进来，
      判据改为与桌面一致的 `phase == Suspended`（D7），命中后 `persist()` 回写；
      **删掉 :229 `is_expired_recoverable_receive` 与 :223-237 的解释性注释**
      （调用点对齐后那条分叉理由不再成立）
- [x] 5.7 `update_session_origin`：Web 无 MCP，实现为写内存 `origin` 字段 + `persist()`
      （不做 `unimplemented!`——端口方法不该有平台空洞）
- [x] 5.8 `PersistentSessionStore::load()`（:79）**移除加载期 reap**：删掉 :89 的 cutoff、
      :106-109 的命中分支、:116-120 的回写循环；模块注释第 26-29 行同步改写
- [x] 5.9 `crates/web/src/node.rs:214-230`：在 `cleanup_recoverable_sessions()` **之后**
      调 `session_store.reap_expired_suspended_receives(SUSPENDED_RECEIVE_RETENTION_SECS)`，
      结果只记日志；注释写明**不调 `cleanup_expired_part_files`** 及原因
      （`OpfsFileAccess::open_or_create_sink` 会造出空文件、`cleanup_sink` 不删文件，design D7）
- [x] 5.10 删除 `crates/web/src/store.rs:663-678` 的本地 `content_root_of`，
      :628 改调 `swarmdrop_transfer::store::content_root_of`（1.8 的新签名）
- [x] 5.11 `crates/web/src/node.rs:139` 的字段类型 `Arc<PersistentSessionStore>`
      改为 `Arc<dyn TransferStore>`（R2 的 Web 侧表现），:175/:182/:219/:250 跟着调整
- [x] 5.12 `crates/web/src/node.rs:639 transfer_history()` 改 `pub async fn`，
      内部改调 `list_transfer_projections().await`（D10）
- [x] 5.13 **新增 wasm 导出** `pub async fn delete_transfer_session(&self, session_id: String) -> Result<(), JsValue>`
      → `self.manager.delete_session(sid)`（走 D4 域方法，非终态被拒绝时错误经
      `WebError` 透出）
- [x] 5.14 **新增 wasm 导出** `pub async fn clear_transfer_history(&self) -> Result<(), JsValue>`
      → `store().clear_all_history()`
- [x] 5.15 `pnpm build:wasm`（在 `docs/` 下）重新生成
      `docs/packages/swarmdrop-web/`，确认 `.d.ts` 里 `transfer_history()` 变
      `Promise<TransferProjection[]>` 且两个新方法出现
- [x] 5.16 `docs/app/app/_components/web-node-bootstrap.tsx:68`：`setHistory` 改 await。
      **`startEventConsumption(node)` 必须仍在 await 之后**——注释里那句「源三先于源一」
      是 `setHistory` 的「已存在的不覆盖」策略（`_lib/store.ts:163-169`）成立的前提
- [x] 5.17 `docs/app/app/_lib/store.ts`：加 `removeProjection(sessionId)` 与
      `clearTerminalProjections()` 两个动作。**selector 纪律**：只返回原始值或
      store 内的稳定引用（`_lib/create-store.ts` 是自研 store，
      `pnpm check:zustand-access` 不覆盖 docs/，没有机器兜底）
- [x] 5.18 `docs/app/app/_components/transfer-activity-panel.tsx`：行内加删除按钮，
      **仅终态与 suspended 可见**（与桌面 `!isActive` 同一判据）；用现有
      `useKeyedAsyncAction`（:138 已有 resume 的同款）承载 pending / error
- [x] 5.19 同文件加「清空记录」入口 + 二次确认（issue #104 明确要求）。
      确认文案写明「只清空已结束的记录；已接收的文件仍在收件箱，不受影响」
- [x] 5.20 `docs/app/app/inbox/page.tsx:8-10` 的分工注释补一句：文件生命周期属收件箱侧，
      传输页删除只删记录（design D5 的前瞻定义）

## 6. 测试

- [x] 6.1 `crates/storage-sql/src/ops.rs` 或 `store.rs` 测试：
      `list_transfer_projections` 返回按 `started_at` **倒序**（造 3 条乱序落库再断言顺序）
- [x] 6.2 同上，Web 侧对应测试（`crates/web` 的 wasm 测试或纯逻辑测试）——
      两端跑同一组断言，这是 D3 的意义所在
- [x] 6.3 `delete_session` 不删收件箱条目：建完成接收会话 → 建收件箱条目 → 删会话 →
      断言 `inbox_items` 行仍在且 `transfer_session_id IS NULL`（钉死
      `ON DELETE SET NULL`，见 migration :29-31）
- [x] 6.4 `TransferManager::delete_session` 对 `phase = Active` 返回 Err、
      对 `Terminal` / `Suspended` 成功（D4）
- [x] 6.5 `clear_all_history` 保留非终态会话（D6）：造 1 终态 + 1 active，清空后断言
      active 仍在
- [x] 6.6 `reap_expired_suspended_receives` 的既有断言（`ops.rs:706` 一带）迁到新调用形态，
      判据保持 `phase = Suspended`
- [x] 6.7 `content_root_of` 合一后的回归：`local_dir` 全一致 → 该目录；
      不一致 / 缺失 → 回退 `save_path`；无 `save_path` → `None`
- [x] 6.8 `crates/core/tests/e2e_transfer.rs` 全绿（:311 与 :1267 两处已改）

## 7. 门禁与验收

- [x] 7.1 `cargo fmt --all`
- [x] 7.2 `cargo check --workspace --all-targets`
- [x] 7.3 `cargo test --workspace`
- [x] 7.4 `cargo clippy --workspace`
- [x] 7.5 `./scripts/check-wasm.sh`
- [x] 7.6 `./scripts/check-wasm.sh --clippy`
- [x] 7.7 根目录 `pnpm exec tsc --noEmit` + `pnpm test`
      （`pnpm check:zustand-access` 只在根 `src/` 有改动时才有意义；本 change 预期
      根 `src/` 只动两处文案）
- [x] 7.8 `docs/` 下 `pnpm build`（静态导出三限制的兜底：新增删除入口不能引入动态段，
      用到 `useSearchParams` 要套 `<Suspense>`）
- [x] 7.9 `mobile/` 下 `pnpm typecheck`
- [ ] 7.10 **手动 — 桌面**：传输页删单条（终态）、删 suspended（确认文案提示断点丢失）、
      清空（进行中那条必须还在）、从历史重新发送
- [ ] 7.11 **手动 — Web**：删单条 → 刷新页面不复活（#104 验收标准）；
      清空 → 收件箱里的文件仍能下载（D5 的分工验证）；
      进行中的会话没有删除按钮，且经控制台直调导出也被拒
- [ ] 7.12 **手动 — 移动**：删单条 / 清空 / 从历史重新发送（含源路径已失效的回退分支）

## 8. 收尾

- [x] 8.1 `dev-notes/knowledge/storage-abstraction.md`：更新「端口覆盖范围」一节 ——
      `SessionStore` 现在覆盖历史管理，`TransferManager::store()` 是宿主取回 store 的
      唯一正路，宿主不得再另存 ORM 连接做传输查询
- [x] 8.2 同文件记下 D5 / D7 两条平台差异（删记录不删文件；Web 不清 `.part`），
      并标注「OPFS 文件清理归 C3」
- [x] 8.3 `dev-notes/knowledge/rust-backend.md`：补一条「端口要有出口」的通用教训 ——
      注入了却拿不回来的依赖会逼出宿主侧的影子副本（本 change 的桌面 `DatabaseConnection`
      State 与 Web 的具体类型字段各是一例）
- [ ] 8.4 关闭 issue #104，回帖写明两条「待定」的最终答案与依据（migration 的
      `ON DELETE SET NULL`、桌面确认文案），以及归给 C3 的部分
