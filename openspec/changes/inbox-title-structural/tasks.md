> **顺序说明**：1 改的是三端共用的 DTO 与领域规则，改完 workspace 会整片编译失败，
> 直到 4 结束才重新变绿。这是刻意的——`primary_file_name` 是破坏性字段变更，
> 让每个消费方在**编译期**红出来，比留一个默认值让它们静默拿到空串可靠得多
> （`delete_finalized_file` 那次「刻意不给默认实现」是同一条判据）。
> 中途不要为了让 `cargo check` 变绿而给字段加临时默认值。

## 1. 领域层：结构化字段 + 规则签名

- [x] 1.1 `crates/transfer/src/inbox.rs`：`InboxItemSummary.title: String` →
      `primary_file_name: Option<String>`；`InboxSearchHit.title` 同改。
      `item_count` 不动（三态判别靠它，见 design 决策 1）
- [x] 1.2 删除 `inbox_title` 及其单测 `title_covers_empty_single_and_multi`
      （展示串移交三端，领域层不再产出散文）
- [x] 1.3 `inbox_matches` 去掉 `title` 入参（四列 → 三列：`source_name` / `files_text` /
      `extracted_text`），同步改文档里「覆盖四列」的表述
- [x] 1.4 `inbox_snippet` 的 `title: &str` → `primary_file_name: Option<&str>`，
      归属判断改为「命中来源名或首文件名 → `None`」（design 决策 3 的对照表是验收依据）
- [x] 1.5 `INBOX_MATCH_CASES` 删掉 `title` 字段与对应语料列；**逐条复核 `expected` 是否仍成立**
      ——「标题命中 2 字中文词」那条现在只剩 `files_text` 可命中，需改语料使其仍覆盖
      「2 字中文词经 trigram 命中」这个原意（把「报告」放进文件名，而不是删掉这条用例）
- [x] 1.6 `InboxFileFacts` 文档补顺序契约：调用方递进来的顺序即条目内文件顺序，
      `primary_file_name` 取第 0 个、`inbox_content_hash` 按此顺序累加（design 决策 6）
- [x] 1.7 `cargo test -p swarmdrop-transfer` 通过（`inbox_content_hash_known_vector`
      必须仍绿——本 change 不许动那条契约）

## 2. Entity + 桌面迁移

- [x] 2.1 `crates/entity/src/inbox_item.rs`：`title: String` → `primary_file_name: Option<String>`
- [x] 2.2 ✅ 已查实：`inbox_item_files` 主键为自增 `id: i32`，回填用 `ORDER BY f.id` 成立
      （见 design 决策 6）。**但必须连带 3.0 一起做**——读取侧当前无 `ORDER BY`，
      不修则回填顺序与运行时顺序无保证一致
- [x] 2.3 新增 **一条** migration `m20260804_000001_inbox_structural_title`：加 `primary_file_name`
      列 + 从 `inbox_item_files` 回填 + 删 `title` 列 + 重建 `inbox_fts`（去 title 列）。
      回填不到的留 `NULL`（异常数据，不发明新展示态）
- [x] 2.4 **两件事必须同条**（原计划拆两条，审查时否掉）：回滚按注册逆序执行，删列的先跑、
      重建索引的读不到 `i.title`，「索引镜像该列」会落到不实现它的人身上
- [x] 2.5 **必须覆写 `use_transaction() -> Some(true)`**：sea-orm 默认只对 Postgres 开事务，
      而这是本仓第一条多语句非幂等迁移——裸跑时中途崩溃会让应用永久起不来（`duplicate column name`），
      且没有恢复路径。带 up/down 测试 + `runs_in_a_transaction` 断言钉住它

## 3. SQL 存储

- [x] 3.0 **既存缺陷**：`crates/storage-sql/src/inbox.rs` 的文件行加载没有任何排序，
      `inbox_content_hash` 的字节级契约实际靠 SQLite 的 rowid 默认顺序兜着。改为构造 facts 前
      显式 `sort_unstable_by_key(|f| f.id)`（Rust 侧排序，`load().with()` 没有直白的排序入口；
      不改变现有哈希，rowid 顺序本就等于 id 升序）。
      注：**回填的正确性并不依赖这条**——`inbox_item_files` 本就按同一顺序插入，两者天然一致
- [x] 3.1 `crates/storage-sql/src/inbox.rs:133` 附近：不再调 `inbox_title`，
      改为从 `facts` 取第 0 个的 `name` 存 `primary_file_name`
- [x] 3.2 FTS 写入（`:197`）去掉 `title` 列
- [x] 3.3 检索 SQL（`:325`）去掉 `fts.title LIKE ?` 一行与对应的绑定参数，
      更新那段注释里「覆盖四列」的表述
- [x] 3.4 `inbox_snippet` 调用点（`:517`）改传 `primary_file_name`
- [x] 3.5 `sql_like_matches_shared_corpus_same_as_inbox_matches` 跟着新语料走；
      `search_finds_item_by_title_and_source_after_insert` 改名并改断言
      （现在是「按文件名与来源名检索」，标题不再是索引列）
- [x] 3.6 `cargo test -p swarmdrop-storage-sql` 通过

## 4. Web 存储

- [x] 4.1 `crates/web/src/inbox.rs:166` 附近：同 3.1
- [x] 4.2 `inbox_matches` / `inbox_snippet` 的调用点跟签名变更
- [x] 4.3 `crates/web/src/idb.rs`：`DB_VERSION` +1；**同改三处**（store 常量 / `DB_VERSION` /
      `onupgradeneeded` 清单）——漏后两处只在运行时报错
- [x] 4.4 不写迁移 / 回填 / 双写（`CLAUDE.md` 既定判据：Web 端无真实用户）
- [x] 4.5 `./scripts/check-wasm.sh` + `./scripts/check-wasm.sh --clippy` 通过
- [x] 4.6 `./scripts/test-wasm.sh` 通过（`crates/web` 的测试进不了 `cargo test --workspace`）

## 5. 三份 bindings 重新生成

- [x] 5.1 桌面：`cargo test -p swarmdrop --test specta_export` → `src/lib/bindings.ts`
- [x] 5.2 Web：**两步且顺序不能反**——先 `cargo test -p swarmdrop-web --features specta
      --test specta_export` 生成 `crates/web/bindings/bindings.ts`，再在 `docs/` 下
      `pnpm build:wasm` 把它经 `include_str!` 烤进 `.d.ts`。
      （`build:wasm` 只是 `wasm-pack build`，本身不产 bindings；顺序反了 `docs/` 的 `tsc`
      看到的是旧签名、会报一堆假错误——本次实际踩过）
- [x] 5.3 移动：重建 uniffi 桥接 → `mobile/packages/swarmdrop-core/src/generated/`
- [x] 5.4 三份**一起提交**（#107 记过：此前三份都早已落后于已提交的 Rust，且无门禁拦它）

## 6. 三端展示串 + i18n

- [x] 6.1 桌面 `src/routes/_app/inbox/index.lazy.tsx`（4 处 `item.title` / `hit.title` /
      `detail.title`）：改为按 `item_count` + `primaryFileName` 生成，走 Lingui
- [x] 6.2 Web `docs/app/app/_components/inbox-views.tsx`（2 处）：同上，
      **翻译宏只能在组件里展开**——`_lib/` 下只许存 `msg` 描述符（`web-app-frontend.md` 硬约束 4）
- [x] 6.3 移动 `mobile/src/components/inbox/inbox-list.tsx`（`:233` 展示、`:83` 本地过滤）、
      `mobile/src/app/inbox/[itemId].tsx:141`、`mobile/src/app/inbox/search.tsx:369`：同上
- [x] 6.4 移动端媒体判断的 `isImageFile(item.title)` / `isVideoFile(item.title)` 改吃
      `primaryFileName`，**并加 `isSingleFileItem` 前置判断**：`isImageFile` 是文件级谓词，
      而它的产物同时喂给图标、「图片」标签与筛选器（都是条目级断言）。用首文件代表一个
      50 文件的混合条目，会让「图片」筛选捞出一堆 zip 和 pdf；桌面 `ItemIcon` 早就是
      「多文件一律归档图标」。**原计划写的「从此按首文件类型」是错的**，审查时纠正
- [x] 6.5 三端各 `i18n:extract`，**补 en / zh-TW 译文**（漏补会静默回落中文，
      正是本 change 要消灭的现象）

## 7. 三道关与收尾

- [x] 7.1 机器门禁全过：`cargo fmt --all` / `cargo check --workspace --all-targets` /
      `cargo test --workspace` / `cargo clippy --workspace` /
      `./scripts/check-wasm.sh [--clippy]` / `./scripts/test-wasm.sh` /
      `pnpm exec tsc --noEmit` / `pnpm test` / `pnpm check:zustand-access`；
      `docs/` 与 `mobile/` 各自 `pnpm typecheck`
- [x] 7.2 `/simplify`
- [x] 7.3 `/code-review`（跨层改动，按并行审查做：领域层与迁移 / 三端存储实现 / 前端展示与 i18n）
- [ ] 7.4 手测三端：切 locale → 收件箱标题跟随（含存量条目）；**多文件条目不按首文件类型
      分类**（桌面归档图标 / 移动不进「图片」筛选）；单文件条目仍按扩展名出图标；
      搜「个文件」不再捞出全部多文件条目；搜非首文件的文件名仍给片段
- [ ] 7.5 主 spec 的同步**由 archive 流程做**，不手改。涉及**三个** capability：
      新增 `inbox-item-presentation`、MODIFIED `inbox-search`（三条 Requirement）、
      MODIFIED `mcp-find-tools`（`McpInboxHit` 已无 title 字段）。
      ⚠️ OpenSpec 的 sync 规则是「保留 delta 未提及的内容」，而本 change **改了一条
      scenario 的标题**（「两字中文词命中标题或文件名」→「…命中文件名」）——
      archive 后要确认旧的那条没有与新的并存
- [x] 7.6 更新知识库 `dev-notes/knowledge/storage-abstraction.md`：补「派生展示串不进检索索引」
      这条判据（决策 2 的推导），以及首文件顺序契约（决策 6）
- [ ] 7.7 关闭 #110；在 #102 下说明「收件箱标题已随本 change 解决」
