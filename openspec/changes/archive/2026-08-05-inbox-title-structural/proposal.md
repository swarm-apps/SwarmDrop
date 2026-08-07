## Why

**主干已在别处落地。** 本 change 原本要做的事——把 `inbox_title` 产出的中文散文
（「空传输」/「`a.pdf` 等 3 个文件」）从领域层与数据库里摘掉、展示串交三端 catalog
——已由 `rust-string-boundary`（commit `c6db98e1`）实现：`inbox_title` 变成
`inbox_primary_file_name`，`inbox_items.title` 改存首个文件名，
`m20260806_000001_inbox_title_to_file_name` 做了回填。

两边的判断一致到函数名都撞了，剩下的差异只是取舍：那边保留列名 `title`、返回空串，
本 change 原计划改名 `primary_file_name` 并返回 `Option<String>`。**改名不做**——
收益是名实相符，代价是在新迁移之上再叠一条改名迁移、三份 bindings 全部重生成、
三端调用点再改一遍。

于是本 change 收窄成四条那边没做、但同一批推导必然指向的收尾。它们各自独立，
且都不改变任何展示行为。

## What Changes

**1. 检索索引删掉 `title` 列**（issue #110 的落点，且**不**新起 `search_text` 列）

`inbox_search_index.title` 现在存首文件名，而同表 `files_text` 是全部文件的
`name` + `relative_path` 拼接——首文件名必然是它的子串。两列做同一个
`LIKE '%needle%'`，前者能命中的后者一个不落。#110 预设「检索需要文本就单起一列」，
实际推导下来那一列没有独立信息。

删列后**检索语义零变化**（散文时代那截「等 N 个文件」噪音已随上一条迁移消失）。
`inbox_matches` 随之四列变三列，`INBOX_MATCH_CASES` 语料同步——其中「2 字中文词」
那条把词挪进文件名而非删掉用例：它守的是「trigram 对 <3 字查询返回空」这个换用
`LIKE` 的原因，与哪一列无关。

**2. Web 端存量脏行**（`develop` 上的现存缺陷）

`crates/web/src/idb.rs` 的 `DB_VERSION` 停在 4，而 `inbox` 行里 `title` 的含义已经变了。
v4 与 v5 的行结构**逐字段相同**，旧行会反序列化成功、然后被前端再拼一次后缀，显示成
「a.pdf 等 3 个文件 等 3 个文件」——无 warn 无报错。`onupgradeneeded` 是唯一知道旧版本号、
因而唯一能丢掉旧行的地方，所以版本号照样要提。

**3. 文件顺序此前没有任何强制**（既存缺陷，与标题无关）

`inbox_content_hash` 逐文件累加，顺序是**跨端去重的字节级契约**（有 known vector 钉着），
`inbox_primary_file_name` 取第 0 个也依赖它。但 `TransferSession::load().with(TransferFile)`
的关系加载没有 `ORDER BY`，整个契约一直靠 SQLite「不加排序时按 rowid 返回」这一实现行为
兜着。加 join、改查询计划或换后端都会静默改掉 `content_hash`，而那是不报错的一类损坏。

**4. 移动端媒体判定用首文件代表整个条目**（既存缺陷）

`isImageFile(item.title)` / `isVideoFile(item.title)` 是**文件级**谓词，产物却是**条目级**
断言——同时喂给图标、「图片」标签与筛选器。散文时代它在多文件条目上恒为假（拿「个文件」
当扩展名）；标题换成首文件名之后反而变成**假阳性**：「封面.jpg + 50 个 zip」会被归成图片、
进「图片」筛选。加单文件前置判断，与桌面 `ItemIcon` 的「多文件一律归档图标」同规。

**非目标**：不改 `inbox_content_hash`（字节级契约）；不改检索匹配算法（仍是大小写不敏感
子串，仍保留「Rust 折 Unicode / SQLite 折 ASCII」那条刻意差异）；不改列名 `inbox_items.title`；
不新增 locale；不碰 `extracted_text` 那一列。

## Capabilities

### New Capabilities

- `inbox-item-presentation`: 条目级的内容类型判定不得由单个文件代表——文件级谓词
  （扩展名判断）只在单文件条目上成立，多文件条目一律走通用形态。

### Modified Capabilities

- `inbox-search`: 检索索引不覆盖由已索引内容派生的展示字段；覆盖面为来源名 + 文件名与
  相对路径 + 抽取正文三列。

## Impact

- **领域层** `crates/transfer/src/inbox.rs`：`inbox_matches` 去掉 `title` 入参（四列 → 三列）；
  `INBOX_MATCH_CASES` 语料去掉 `title` 列并复核每条 `expected`。
  `inbox_snippet` **不动**——它用标题判「命中的东西是不是条目行上已经显示着的」，那是
  归属判断而非命中判断。
- **实体** `crates/entity/src/inbox_search_index.rs`：删 `title` 字段。
- **迁移** 新增 `m20260807_000001_drop_search_index_title`（`drop_column`，带 up/down 测试）。
  顺带纠正 `crates/migration/src/lib.rs` 里「SQLite 不支持删列」的过时注释——那是 3.35
  之前的事实。
- **SQL 存储** `crates/storage-sql/src/inbox.rs`：索引写入与检索 SQL 去 `title` 列；
  文件行加载显式按 `id` 排序；新增 `title_is_not_indexed`（用只存在于 `inbox_items.title`
  的哨兵词钉住，否则测不出来）。
- **Web 存储** `crates/web/src/inbox.rs` + `idb.rs`：`inbox_matches` 调用点跟签名；
  `DB_VERSION` 4 → 5 并在 `onupgradeneeded` 里按 `old_version` 丢掉旧 `inbox` store。
- **移动前端** `mobile/src/components/inbox/inbox-list.tsx`：`isImageLike` / `isVideoLike`
  加 `isSingleFileItem` 前置。
- **零改动确认**：`inbox_content_hash`、`inbox_files_text`、`inbox_primary_file_name`、
  `inbox_snippet`、`is_completed_receive`、`INBOX_SEARCH_LIMIT`、三份 bindings、
  三端 i18n catalog、`inbox_items.title` 列本身均不改。
