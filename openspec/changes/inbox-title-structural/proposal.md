## Why

`crates/transfer/src/inbox.rs::inbox_title` 产出「空传输」/「`a.pdf` 等 3 个文件」这样的**中文散文**，
直接写进 `inbox_items.title` **落库**，并进 `inbox_fts.title` 参与检索。前端拿到的
`InboxItemDetail.item.title` 已经是成品字符串，Lingui 无从下手。

这与本仓早已确立的一条原则**正面冲突**——`localize-backend-strings` 的决策 1：

> 后端 / core 只发「稳定语义码 + 结构化参数」，永不产出本地化散文。翻译发生在呈现边缘。

那次 change 把错误消息、托盘、系统通知三桶都收拢到这条原则下，`inbox_title` 是**漏网的第四桶**，
且是最深的一桶：另外三桶只在运行时产出散文，它还把散文**持久化**了。

后果有三层，一层比一层不显眼：

1. **#102 做完之后收件箱那一栏仍然是中文**——应用区其余部分接了 Lingui，标题接不了。
2. **移动端已经在拿 title 当文件名用**（`mobile/src/components/inbox/inbox-list.tsx:393-397`
   的 `isImageFile(item.title)` / `isVideoFile(item.title)`）。这依赖一条从未写下来的隐含契约
   「单文件条目的 title 就是文件名」，**在多文件条目上本来就是坏的**——title 是「`a.pdf` 等 3 个文件」，
   扩展名匹配不上，媒体图标静默退化成通用图标。
3. **FTS 的 title 列在制造检索噪音**。真实 title 只有三种形态，逐一比对
   `inbox_files_text`（`"{name} {relative_path}"` 逐文件拼接）就会发现它对检索**完全冗余**，
   而多出来的那截还有害：所有多文件条目的 title 都含「个文件」，搜「文件」会命中全部。

扩散面还在扩大：这条规则原本住在 `crates/storage-sql`，`inbox-store-port-completion`
为三端共用把它上提到了共享领域层，于是现在有两个调用方（`crates/storage-sql/src/inbox.rs:133`、
`crates/web/src/inbox.rs:166`），每多接一端就多一批存量脏数据。

## What Changes

**领域层只给结构，展示串交呈现边缘生成。** 与 `localize-backend-strings` 的决策 1 同构，
不是新原则，是把已有原则贯彻到最后一处。

- **`inbox_title` 删除。** `InboxItemSummary.title: String` 替换为
  `primary_file_name: Option<String>`；`item_count: i32` 本来就在，两者合起来足以让任何一端
  还原现有的三种展示形态。三端各用自己的 i18n（Web / 桌面 Lingui、移动 Lingui）生成展示串。

- **FTS 的 `title` 列直接删掉，不新起 `search_text` 列。** issue #110 预设的方向是「检索需要文本
  就单起一列」，实际推导下来那一列是多余的：单文件条目的 title 被 `files_text` 完全覆盖，
  多文件条目只多出「等 N 个文件」，空条目只多出「空传输」——两者都是噪音而非功能。
  删掉之后检索语义的唯一变化是「搜『空传输』『个文件』不再命中」，那是修复不是回退。
  连带 `inbox_matches` 去掉 `title` 入参、`INBOX_MATCH_CASES` 语料去掉 `title` 列。

- **`inbox_snippet` 的归属判断改吃首文件名。** 它现在用 title 判断「命中的东西是不是条目行上
  已经显示着的」，换成 `primary_file_name` 之后语义**更准**：多文件条目下命中第二个文件仍然给片段，
  命中首文件名不给——与现在等价，但不依赖成品串。

- **存量数据按端分办，且都不需要解析旧的中文串。** 桌面（有真实用户）走 migration，
  从 `inbox_item_files` 回填 `primary_file_name`——首文件名是**结构数据**，能从现存文件行精确重建。
  Web 直接换 schema（`DB_VERSION` +1，无迁移无回填，依 `CLAUDE.md` 的既定判据）。

- **顺带修掉移动端的媒体判断。** `isImageFile` / `isVideoFile` 此前吃 `item.title`，
  多文件条目的「a.jpg 等 3 个文件」被按扩展名「个文件」判断，恒为假。改吃
  `primaryFileName`，**但只对单文件条目判**——`isImageFile` 是文件级谓词，用首文件代表
  一个混合内容的多文件条目，会让「图片」筛选捞出一堆 zip 和 pdf。桌面 `ItemIcon` 早就是
  「多文件一律归档图标」，两端就此同规。

**非目标**：不改 `inbox_content_hash`（跨端去重契约，字节级不可动）；不改检索的匹配算法
（仍是大小写不敏感子串，仍保留「Rust 折 Unicode / SQLite 折 ASCII」那条刻意差异）；
不新增 locale；不碰 `extracted_text` 那一列。

## Capabilities

### New Capabilities

- `inbox-item-presentation`: 收件箱条目的展示标题由各端从结构化字段（首文件名 + 文件数）按当前
  locale 生成；领域层与持久化层不产出、不存储任何本地化散文。

### Modified Capabilities

- `inbox-search`: 检索索引不再覆盖条目标题列（该列被文件名文本完全覆盖，且为多文件条目引入噪音）；
  索引覆盖面收窄为来源名 + 文件名 + 相对路径 + 抽取正文。

## Impact

- **领域层** `crates/transfer/src/inbox.rs`：删 `inbox_title`；`InboxItemSummary` /
  `InboxSearchHit` 的 `title` 字段换成 `primary_file_name`；`inbox_matches` 与 `inbox_snippet`
  签名变更；`INBOX_MATCH_CASES` 语料改列。
- **SQL 存储** `crates/storage-sql/src/inbox.rs`：FTS 写入与检索 SQL 去掉 title 列；
  条目写入改存 `primary_file_name`。
- **Web 存储** `crates/web/src/inbox.rs` + `idb.rs`：同上，`DB_VERSION` +1。
- **迁移** `crates/migration/`：新增**一条** `m20260804_000001_inbox_structural_title`
  ——加列 + 回填 + 删列 + 重建 `inbox_fts`。两件事必须同条：拆开后回滚按注册逆序执行，
  删列的先跑、重建索引的读不到 `i.title`，「索引镜像该列」就落到了不实现它的人身上。
- **实体** `crates/entity/src/inbox_item.rs`：`title` → `primary_file_name`。
- **三端前端**：桌面 `src/routes/_app/inbox/`、Web `docs/app/app/_components/inbox-views.tsx`、
  移动 `mobile/src/components/inbox/` + `mobile/src/app/inbox/`——各自新增一处标题生成，
  移动端另修媒体判断。
- **bindings**：`src/lib/bindings.ts`、`crates/web/bindings/bindings.ts`、
  `mobile/packages/swarmdrop-core/src/generated/` 三份重新生成。
- **i18n**：三端各加 3 条文案（空 / 单 / 多），源 locale `zh`，需补 `en` / `zh-TW`。
- **零改动确认**：`inbox_content_hash`（跨端去重契约）、`inbox_files_text`、`is_completed_receive`、
  `INBOX_SEARCH_LIMIT`、传输链、端口 trait 的方法签名均不改。
