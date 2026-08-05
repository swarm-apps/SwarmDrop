## Context

`inbox_title` 是本仓「后端发码、边缘翻译」原则的最后一处缺口，但它比
`localize-backend-strings` 处理的那三桶多两个维度，方案不能照抄：

| | 错误 / 托盘 / 通知（已解决） | `inbox_title`（本 change） |
|---|---|---|
| 散文产出时机 | 运行时 | 运行时 **+ 落库** |
| 有没有第二消费者 | 没有，只给人看 | **有**：`inbox_fts.title` 参与检索 |
| 存量数据 | 不存在 | 桌面有真实用户 |

多出来的两条决定了本 change 的重心不在「怎么翻译」（那部分是 `localize-backend-strings`
已经趟过的路），而在**「拆掉一个既是展示串又是检索文本的字段」**。

## Goals / Non-Goals

**Goals**
- 收件箱标题随当前 locale 变化，三端一致，存量条目也跟着变。
- 领域层与持久化层零本地化散文——与错误 `kind`、通知语义枚举同构。
- 移动端的媒体类型判断在多文件条目上不再静默退化。

**Non-Goals**
- 不改 `inbox_content_hash`（跨端去重契约，`inbox_content_hash_known_vector` 钉死字节序）。
- 不改匹配算法本身：仍是大小写不敏感子串，仍保留「Rust 折 Unicode / SQLite 只折 ASCII」
  那条**刻意**的两端差异（理由见 `inbox_matches` 文档，不在本 change 重新论证）。
- 不碰 `extracted_text` 列，不新增 locale，不做复数语法（见决策 4）。

## Decisions

### 决策 1：结构选 `primary_file_name + item_count`，不是「标题模板 + 参数包」

现有三种展示形态：

```rust
[]              => "空传输"
[file]          => file.name
[first, ..]     => format!("{} 等 {} 个文件", first.name, files.len())
```

三种形态的可变部分合起来只有两个值：**首文件名**与**文件数**。而 `item_count: i32`
**本来就在** `InboxItemSummary` 上（也在 `inbox_items` 表里），所以真正要加的只有一个
`primary_file_name: Option<String>`。

考虑过但否决的形状：

| 形状 | 否决理由 |
|---|---|
| `title_kind: enum { Empty, Single, Multi }` + 参数 | `item_count` 已经能判别三态（0 / 1 / >1），枚举是它的冗余投影，两者会漂移 |
| `title_params: serde_json::Value` | 把结构退化成无类型袋子，前端拿不到编译期保证 |
| 保留 `title` 但存 i18n key（如 `"inbox.title.multi"`）| key 落库后就是新的 wire 契约，改文案要迁移数据；且三端各自的 catalog 键空间不同 |

`Option` 而不是 `String`：**空条目没有首文件名，这是真实状态而不是空串**。这与
`inbox_matches` 的 `extracted_text: Option<&str>` 同一条纪律——「这一端/这一条没有这个东西」
和「有但是空的」必须在类型上分得开。空串会让「空传输」和「文件名恰好是空字符串」撞在一起。

### 决策 2：FTS 的 `title` 列删掉，不新起 `search_text` 列

issue #110 预设的方向是「FTS 需要文本就单起一个 `search_text` 列，与展示解耦」。
**推导下来那一列不需要存在**，理由是纯粹的包含关系。

设 `T = inbox_title(files)`，`F = inbox_files_text(files)`，后者定义为
`files.map(|f| format!("{} {}", f.name, f.relative_path)).join(" ")`。逐形态比对：

| files | `T` | `T` 的子串是否都是 `F` 的子串 | `T` 独有的可搜文本 |
|---|---|---|---|
| `[]` | `"空传输"` | 否（`F` = `""`） | `"空传输"` |
| `[f]` | `f.name` | **是**（`F` 以 `f.name` 开头） | 无 |
| `[first, ..]` | `"{first.name} 等 {N} 个文件"` | 部分（`first.name` 被覆盖） | `" 等 N 个文件"` |

所以删掉 title 列，检索行为的变化**恰好只有两条**：

1. 搜「空传输」不再命中零文件条目；
2. 搜「个文件」「等 3 个文件」不再命中多文件条目。

第 2 条是**修复**：所有多文件条目的 title 都含「个文件」，用户搜「文件」会把它们全部捞出来
——一个与用户意图完全无关的结果集。第 1 条是失去了一个没人会用的检索词，且零文件条目
本身就是异常数据。

**判据可以复用**：一个派生字段该不该进检索索引，看它相对于已索引字段有没有**独立的信息**。
`title` 是 `files` 的纯函数，且是有损投影——它不可能带来 `files_text` 没有的信息，只可能
带来模板噪音。同理，将来任何「由已索引字段派生的展示串」都不该进索引。

代价是 `inbox-search` spec 要改一条（索引覆盖面），以及 SQL 的 `LIKE` 从四列变三列——
`INBOX_MATCH_CASES` 的共享语料仍然是两端同义的锚点，只是少一列。

### 决策 3：`inbox_snippet` 的归属判断改吃首文件名，语义反而更准

`inbox_snippet` 现在的第一个入参是 title，用途**不是**生成片段，而是判断
「命中的东西是不是条目行上已经显示着的」——命中标题或来源名就返回 `None`，不重复渲染。

title 拆掉之后不能简单删掉这个入参：删了之后「用户搜首文件名」会走进 files 循环、
生成一个内容与标题行重复的片段，正是这个函数想避免的。

改成收 `primary_file_name: Option<&str>`：

```rust
pub fn inbox_snippet(
    query: &str,
    primary_file_name: Option<&str>,
    source_name: &str,
    files: &[InboxHitFile],
) -> Option<String>
```

与现状的行为对照：

| 条目 | 用户搜 | 现状（吃 title） | 新（吃首文件名） |
|---|---|---|---|
| 单文件 `a.pdf` | `a.pdf` | `None`（命中 title） | `None`（命中首文件名） |
| 多文件 `a.pdf` + `b.pdf` | `a.pdf` | `None`（title 含它） | `None` |
| 多文件同上 | `b.pdf` | 给片段 | 给片段 |
| 任意 | 「个文件」 | **`None`**（命中 title 的模板部分） | 走 files，不命中 → `None` |

前三行完全等价，第四行现状是个假归属判断——它把「命中了模板文字」误判成「用户要找的
东西在条目行上可见」。新形态下这种查询根本不该命中（决策 2），两条修复是同一个根因。

### 决策 4：三端各自生成展示串，不共享模板

三端各写一次这段：

```
!primaryFileName           → t`空传输`     // 先判缺席，零文件条目与异常数据都落这里
itemCount <= 1             → primaryFileName
否则                        → t`${primaryFileName} 等 ${itemCount} 个文件`
```

判别**先看首文件名再看文件数**，顺序是有意的：`item_count >= 2` 却没有文件行的异常数据
必须落到「空传输」，而不是插值出一个 `" 等 3 个文件"`（带前导空格的畸形串）。
迁移 `down()` 里还原旧标题的 CASE 用的是同一个顺序。

**不把它收进 `packages/shared-view`**，尽管那个包正是「三端共享的纯视图逻辑」。判据是
该包 README 的归属线：共享的是**逻辑**，而这里逻辑只有一个三分支判别，真正的内容是**文案**，
而文案本就分属三套独立 catalog（桌面 Lingui 5、Web Lingui 6、移动 Lingui 6，
`CLAUDE.md` 明确写了「三份独立 catalog」）。把三分支判别抽走、文案留在各端，
等于为省 3 行代码引入一个跨 workspace 依赖。

**复数语法明确不做。** 中文源 locale 无复数变化，`en` 的「1 file / 3 files」由 `item_count > 1`
这个分支天然规避（等于 1 的分支不走模板）。真需要时 Lingui 的 `plural` 宏可以就地加，
不影响本 change 的数据形状。

**Web 端有一条额外约束**（`web-app-frontend.md` 的第 4 条硬约束）：翻译宏只能在组件里展开，
`_lib/` 下的纯函数只能存 `msg` 描述符。所以 Web 的标题生成要么写在组件里，
要么在 `_lib/` 里存描述符、由组件 `t(...)` 展开——**不能**写成一个直接返回成品字符串的
模块级函数，那正是这次要消灭的形态在前端的翻版。

### 决策 5：存量数据——桌面回填结构，Web 直接换

关键判断：**回填的是首文件名，不是标题**。首文件名能从 `inbox_item_files` 精确重建，
不需要解析任何旧的中文串（那才是会出错的做法：「`a.pdf` 等 3 个文件」反解析要处理文件名
本身含「 等 」的情况）。

```sql
-- 桌面 migration：加列 + 从文件行回填
UPDATE inbox_items SET primary_file_name = (
    SELECT f.name FROM inbox_item_files f
    WHERE f.inbox_item_id = inbox_items.id
    ORDER BY f.id LIMIT 1
);
```

`ORDER BY f.id` 是有依据的，见决策 6。

**回填不到的情况**（文件行已不存在）留 `NULL`，展示退化为「空传输」——与
`item_count` 的实际值可能不一致，但那种条目本身已经是异常数据（`item_count > 0` 却没有文件行），
本 change 不为它发明新的展示态。

**Web 侧直接换 schema**（`DB_VERSION` +1，`onupgradeneeded` 建新形状），不写迁移 / 回填 / 双写。
依据是 `CLAUDE.md` 与 `storage-abstraction.md` 已记录的既定判据：Web 端没有真实用户，
「保住旧数据」收益为零。**注意 IndexedDB 加/改 store 要同改三处**（store 常量 /
`DB_VERSION` / `onupgradeneeded` 清单），漏后两处只在运行时报错。

FTS 那张虚表**整表重建**（`DROP` + `CREATE` + 从 `inbox_items` / `inbox_item_files` 重灌），
不做 `ALTER`——fts5 虚表改列本来就要重建，而重灌的数据源就是那两张真表，不依赖旧索引内容。
`m20260630_000001_inbox_fts` 已经有一次同形状的回填可以照抄。

### 决策 6：把「首文件」的顺序契约显式化——并补上 SQL 侧缺失的 `ORDER BY`

`inbox_title` 的 `[first, ..]` 依赖 `files` 切片的顺序，而这个顺序此前**没有任何地方写明**。

它不是本 change 引入的新依赖——`inbox_content_hash` 逐文件累加
`relative_path ‖ 0x00 ‖ checksum ‖ size_le`，**同样依赖这个顺序**，而它是跨端去重的唯一判据、
有 known vector 钉着。顺序契约一直存在，只是从来没被写下来。

**排查时发现的更要紧的一件事：SQL 侧根本没有强制这个顺序。**
`crates/storage-sql/src/inbox.rs:110` 的 `std::mem::take(&mut loaded.files)` 拿的是 sea-orm
关系加载 `TransferSession.with(TransferFile)` 的结果——注意**源头是 `transfer_files`
而不是 `inbox_item_files`**（收件箱条目由传输文件行建成，`inbox_item_files` 是随后按同一
顺序插入的产物）。而整个文件里唯一的 `order_by` 在 `:272`（条目按 `received_at` 倒序），
**文件行没有任何排序**。也就是说 `inbox_content_hash` 的字节级契约目前实际依赖
SQLite「不加 `ORDER BY` 时按 rowid 返回」这一实现行为——实践上稳定，规范上不保证，
且一旦将来加了 join、改了查询计划或换了后端就会静默改变哈希。

所以本 change 做两件事，第二件是顺手补的既存缺陷：

1. **写下来**：在 `InboxFileFacts` 的文档里写明「调用方递进来的顺序即条目内文件顺序，
   `primary_file_name` 取第 0 个，`inbox_content_hash` 按此顺序累加」，两端的构造点各自
   说明顺序从哪来（SQL 侧 = `transfer_files.id` 升序，Web 侧 = 条目内序号 0..n，
   后者在 `storage-abstraction.md` 已有记录）。
2. **强制它**：构造 facts 之前显式 `file_rows.sort_unstable_by_key(|f| f.id)`
   （`id` 是唯一主键，稳定排序的临时缓冲不表达任何东西）。用 Rust 侧排序而不是
   给关系加载挂 `ORDER BY`，是因为 sea-orm 的 `load().with()` 没有直白的排序入口，而这里
   的规模（一次接收的文件数）让排序成本可忽略；写成一行代码也比藏在 ORM 调用链里更容易读。
   这不改变现有数据的哈希（rowid 顺序本来就等于 id 升序），但把一条靠实现行为兜着的契约
   变成了代码里的约束。

**顺带澄清一个本可以误判的点**：迁移回填用的是 `inbox_item_files.id` 升序，它与运行时
**天然一致**——因为 `inbox_item_files` 本来就是按 `file_rows` 的顺序插入的，自增 id 即那个
顺序。所以回填的正确性不依赖第 2 件事。第 2 件事保护的是 `inbox_content_hash` 的跨端一致性，
那是个独立于本 change 的既存风险，只是在排查顺序契约时一并暴露了。

## Risks / Open Questions

- **三份 bindings 必须一起重生成并提交。** `InboxItemSummary` / `InboxSearchHit` 改字段会同时
  影响 specta（桌面）、wasm-bindgen（Web）、uniffi（移动）三条边界。#107 已经点名过这个坑：
  三份 bindings 此前都早已落后于已提交的 Rust，且**没有任何门禁拦它**。本 change 的字段是
  破坏性的，漏生成会在前端表现为「字段不存在」而不是编译错误。

- **移动端媒体判断的改动会改变可见行为。** `isImageFile(item.title)` → `isImageFile(primaryFileName)`
  之后，多文件条目**仍然**不按文件类型分类——这与最初的设想相反，理由见下。
  真正变化的是单文件条目：此前 `item.title` 恰好等于文件名所以能判对，改动后判据显式了。

  最初写的是「多文件条目从通用图标变成按首文件类型的图标，这是修复」。**那是错的**：
  `isImageFile` 是**文件级**谓词，而移动端把它的产物同时喂给图标、「图片」标签和
  「图片」筛选器——三者都是关于**整个条目**的断言。一个 50 文件的混合条目只因 `file[0]`
  是 `.jpg` 就被计入「图片」筛选，用户点进去看到一堆 zip 和 pdf；而桌面 `ItemIcon`
  对 `count > 1` 一律出归档图标，同一条数据两端会分叉。现在移动端加了 `isSingleFileItem`
  前置判断，与桌面同规，并在 spec 里补了对应的 Requirement——原 spec 只对**标题**写了
  「三端一致」、对**分类**没写，正是那个缺口让两端真的不一致了。

- **`inbox-search` spec 的索引覆盖面是对外契约的一部分**（spec.md:8 明确列了 `inbox_items.title`）。
  删列要同步改 spec，否则规格与实现从此错位——这正是 `inbox_matches` 文档里记过的那种
  「自称规范定义、实际多匹配一列」的错位，不要在同一个地方犯第二次。

- ~~**桌面回填的 `ORDER BY f.id`** 是否成立~~ **已查实**：`crates/entity/src/inbox_item_file.rs`
  的主键是 `#[sea_orm(primary_key)] pub id: i32`，未写 `auto_increment = false`
  （对照 `inbox_item.rs` 的 Uuid 主键显式关掉了自增），即标准自增整数主键，`ORDER BY f.id`
  等于插入顺序，而插入顺序就是运行时算首文件名用的那个顺序。回填与运行时天然一致。

- **`primaryFileName` 在三端的 TS 类型不是同一种缺席。** specta（桌面 / Web）生成
  `string | null`，uniffi（移动）生成可选属性 `primaryFileName?: string`。移动端那个 hook
  因此收 `string | undefined`（两个调用点都是 uniffi 生成的类型，不会是 `null`）。
  这不是可以统一掉的东西
  ——两套 codegen 对 `Option<T>` 的映射约定本来就不同。

- **不阻塞 #102，但 #102 的验收要等它。** 应用区其余部分的 i18n 已经落地，收件箱标题是
  最后一处中文。两个 change 可以并行推进，合并顺序不敏感。
