> **本 change 已收窄。** 原计划的主干（`inbox_title` 删除、`primary_file_name` 改名、
> 三端展示串 + i18n、bindings 三份重生成）已由 `rust-string-boundary` 的 `c6db98e1` 实现，
> 相应任务不再重复；理由与取舍见 `proposal.md`。剩下的四条各自独立，可任意顺序做。

## 1. 检索索引删掉冗余的 title 列

- [x] 1.1 `crates/transfer/src/inbox.rs`：`inbox_matches` 去掉 `title` 入参（四列 → 三列：
      `source_name` / `files_text` / `extracted_text`），同步改文档里「覆盖四列」的表述
- [x] 1.2 `InboxMatchCase` 去掉 `title` 字段与语料对应列，**逐条复核 `expected`**
      ——「标题命中 2 字中文词」那条现在只剩 `files_text` 可命中，把「报告」挪进文件名
      使其仍覆盖「2 字中文词经 trigram 命中」这个原意，而不是删掉用例；
      「标题与文件文本同时命中」改为「来源名与文件文本同时命中」以保住「多列命中仍是
      一次命中」的覆盖
- [x] 1.3 `inbox_snippet` **不动**：它用标题判归属（「命中的是不是条目行上已显示的东西」），
      与命中判断是两件事。文档里补一句说明，免得下一个人顺手也把它删了
- [x] 1.4 `crates/entity/src/inbox_search_index.rs`：删 `title` 字段 + 文档说明为什么没有
- [x] 1.5 新增 `m20260807_000001_drop_search_index_title`：`drop_column`；
      down 加回空列**不回填**（`files_text` 覆盖同一份文本，回填只是把冗余写实一遍）
- [x] 1.6 迁移带 up/down 测试：`PRAGMA table_info` 断言列的增删。
      **它同时钉着「捆绑的 SQLite 支持 `DROP COLUMN`」**——不支持要在这里红，
      而不是在用户升级时红
- [x] 1.7 顺带纠正 `crates/migration/src/lib.rs` 的「删列 SQLite 不支持」——3.35（2021）起可用
- [x] 1.8 `crates/storage-sql/src/inbox.rs`：索引写入去 `set_title`；检索 SQL 去
      `s.title LIKE ?` 一行与对应绑定参数；注释里「覆盖四列」改三列
- [x] 1.9 新增 `title_is_not_indexed`：**必须用只存在于 `inbox_items.title` 的哨兵词**
      ——那一列的内容本来就在 `files_text` 里，把它加回索引不会让任何现有断言变红
- [x] 1.10 `search_finds_item_by_title_and_source_after_insert` 改名为
      `..._by_file_name_and_source_...`，注释更正为「命中来自 `files_text`，
      但片段的归属判断仍看标题」
- [x] 1.11 `crates/web/src/inbox.rs`：`inbox_matches` 调用点跟签名
- [x] 1.12 `cargo test -p swarmdrop-transfer` / `-p swarmdrop-storage-sql` / `-p migration` 通过
      （`inbox_content_hash_known_vector` 必须仍绿——本 change 不许动那条契约）

## 2. Web 端存量脏行

- [x] 2.1 `crates/web/src/idb.rs`：`DB_VERSION` 4 → 5
- [x] 2.2 `install_upgrade_handler` 按 `old_version` 丢掉旧 `inbox` store，
      `old_version == 0`（新库首建）与拿不到事件时都不丢——多留脏行只是难看，误删是丢数据
- [x] 2.3 `crates/web/Cargo.toml` 加 `IdbVersionChangeEvent` feature（读 `old_version` 要它）
- [x] 2.4 文档写清**为什么不能靠反序列化失败过滤**：v4/v5 行结构逐字段相同，
      旧行会成功读回来、被前端再拼一次后缀，无 warn 无报错
- [x] 2.5 `./scripts/check-wasm.sh [--clippy]` + `./scripts/test-wasm.sh` 通过
- [x] 2.6 `pnpm build:wasm` 重新生成入库的 wasm 产物

## 3. 文件顺序契约

- [x] 3.1 `crates/storage-sql/src/inbox.rs`：构造 facts 前按 **`file_id`** 排序
      （`HasMany` 是 enum，先 `into_iter().collect()` 摊成 Vec——是移动不是深拷）。
      **不是主键 `id`**：那是本地自增代理键，`file_id` 才是协议层定义、两端共有的序号
- [x] 3.2 **`crates/web/src/inbox.rs` 必须同步排序**——单边加排序反而制造跨端分叉：
      改动前两端都用 manifest 顺序（一致），只改一侧会让乱序 offer 在两端算出不同的
      `content_hash`。顺序契约写进 `InboxFileFacts` 文档（类型表达不了）
- [x] 3.3 两端各加「同一批文件、manifest 排列相反 → 指纹相同」的测试，
      并**实际验证它能抓到回归**（临时删掉排序看它是否变红）。
      此前没有任何测试覆盖「行取出顺序」——`inbox_content_hash_known_vector` 直接构造
      facts，根本不经过存储层

## 4. 移动端媒体判定

- [x] 4.1 `isImageLike` / `isVideoLike` 加 `isSingleFileItem` 前置判断
- [x] 4.2 注释说明「文件级谓词 vs 条目级断言」。**别写成与桌面「同规」**——桌面是
      `count > 1` 出归档图标，这里是 `=== 1` 才判扩展名，对 `itemCount === 0` 两端图标
      不同（都不会认成图片/视频，而那正是这条判据要保证的）

## 5. 三道关与收尾

- [x] 5.1 机器门禁：`cargo fmt --all` / `cargo test --workspace` / `cargo clippy --workspace` /
      `./scripts/check-wasm.sh [--clippy]` / `./scripts/test-wasm.sh`（23 passed）
- [x] 5.2 前端门禁：桌面 `pnpm exec tsc --noEmit` + `pnpm test` + `pnpm check:zustand-access`；
      `docs/` 与 `mobile/` 各自 `pnpm typecheck`
- [x] 5.3 `/simplify` 与 `/code-review`
- [x] 5.4 手测 **未做**（需真机 / 真实存量库），archive 时确认四条都另有覆盖：
      - 移动端媒体判定 → `isSingleFileItem` 的类型前提经代码审查确认（`itemCount` 是
        uniffi 的 `number` 而非 bigint，`=== 1` 成立），`pnpm typecheck` 通过
      - 检索不再命中标题 → `title_is_not_indexed`（哨兵词，删了索引列的守卫会红）
      - 非首文件命中仍给片段 → `search_snippet_present_when_only_file_path_matches`（既有）
      - Web v4 → v5 升级 → `STORES` 表的逐版本真值表经审查推演（v4 库只丢 `inbox`，
        v1/v2/v3 删的都是尚不存在的 store）。**真实存量库上的行为仍未观测过**
- [x] 5.5 更新知识库：`dev-notes/knowledge/storage-abstraction.md`（派生展示串不进检索索引、
      Web 换字段含义也要提 `DB_VERSION`、文件顺序契约）、
      `mobile/dev-notes/knowledge/rust-bridge.md`（uniffi 与 specta 对 `Option<T>` 的映射差异，
      锚点换成真实存在的 `root_path`）
- [x] 5.6 主 spec 同步**由 archive 流程做**，不手改。⚠️ OpenSpec 的 sync 规则是「保留 delta
      未提及的内容」，而本 change 改了 `inbox-search` 里一条 scenario 的标题——archive 后
      要确认旧的那条没有与新的并存
- [ ] 5.7 关闭 #110；在 #102 下说明收件箱标题已由 `rust-string-boundary` 解决
