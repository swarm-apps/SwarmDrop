## Context

本 change 的主干已由 `rust-string-boundary`（`c6db98e1`）实现——两边独立推导出了同一个
结论，连函数名都撞成了 `inbox_primary_file_name`。剩下的是**同一批推导必然指向、但那边
没做**的四条收尾，以及一条要主动放弃的改名。

背景仍值得留一句：`inbox_title` 是「后端发码、边缘翻译」原则的最后一处缺口，它比
`localize-backend-strings` 处理的那三桶多两个维度——

| | 错误 / 托盘 / 通知 | `inbox_title` |
|---|---|---|
| 散文产出时机 | 运行时 | 运行时 **+ 落库** |
| 有没有第二消费者 | 没有 | **有**：检索索引的 title 列 |
| 存量数据 | 不存在 | 桌面有真实用户 |

多出来的两条正是本 change 剩下要处理的：**第二消费者**（决策 2）与**存量数据**（决策 3）。

## Goals / Non-Goals

**Goals**
- 检索索引不再收录可由已索引内容完全派生的展示字段。
- Web 端旧 IndexedDB 行不再以「a.pdf 等 3 个文件 等 3 个文件」的形态存活。
- 文件顺序契约显式化——`inbox_content_hash` 不再靠 SQLite 的默认行序兜着。
- 移动端媒体判定不再用首文件代表整个条目。

**Non-Goals**
- 不改 `inbox_content_hash`（`inbox_content_hash_known_vector` 钉死字节序）。
- 不改匹配算法：仍是大小写不敏感子串，仍保留「Rust 折 Unicode / SQLite 只折 ASCII」
  那条**刻意**的两端差异（理由见 `inbox_matches` 文档，不在这里重新论证）。
- 不碰 `extracted_text` 列，不新增 locale，不动三份 bindings 与三端 catalog。

## Decisions

### 决策 0：接受 develop 的 `title: String`，放弃改名 `primary_file_name: Option<String>`

两处差异各自都有道理：`Option` 让「没有文件」与「文件名恰好是空串」在类型上分得开，
与 `inbox_matches` 的 `extracted_text: Option<&str>` 同一条纪律；列名 `primary_file_name`
名实相符，而 `title` 存文件名是笔糊涂账。

但代价是**在已合并的迁移之上再叠一条改名迁移**、三份 bindings 全部重生成、三端调用点
再改一遍——为一次读代码时的愣神，付一整轮跨端改动与一次 schema 变更。不值。

**补偿**：把「`item.title` 是首文件名而非拼好的标题」写进
`dev-notes/knowledge/storage-abstraction.md`，并在 `entity::inbox_search_index` 的文档里
说明它为什么没有对应的索引列。名实不符必须有个地方讲清楚，否则下一个人会照着列名
去写 `isImageFile(item.title)`——那正是决策 4 要修的东西。

### 决策 1：检索索引删掉 `title` 列，不新起 `search_text` 列

issue #110 预设的方向是「检索需要文本就单起一个 `search_text` 列，与展示解耦」。
**推导下来那一列不需要存在**，理由是纯粹的包含关系。

设 `T` = 条目标题、`F = inbox_files_text(files)`（定义为
`files.map(|f| format!("{} {}", f.name, f.relative_path)).join(" ")`）：

| 阶段 | `T` | `T` 独有的可搜文本 |
|---|---|---|
| 散文标题时代 | `""`/`f.name`/`"{first} 等 {N} 个文件"` | 「空传输」、「 等 N 个文件」 |
| 结构化之后（现在） | 首文件名 | **无**（`F` 必以首文件名开头） |

两个阶段删这一列的理由不同：散文时代删它是**消噪音**（所有多文件条目的 title 都含
「个文件」，用户搜「文件」会把它们整批捞出来）；而到真正删的时候散文已经不在了，
删它是**消冗余**——所以本次改动的检索语义**一条都没变**。

**判据可以复用**：一个派生字段该不该进检索索引，看它相对于已索引字段有没有**独立信息**。
纯函数派生且有损的投影不可能带来新信息，只可能带来模板噪音。

**这条判据不适用于 `inbox_snippet`。** 它也读标题，但问的是「命中的东西是不是条目行上
已经显示着的」——那是**归属判断**，不是命中判断，删了会让「搜首文件名」多出一条与标题
一字不差的片段行。签名因此保持不动，并在文档里写明理由，免得下一个人顺手也删掉。

**测试必须用哨兵词。** `title_is_not_indexed` 拿一个只存在于 `inbox_items.title` 的字符串
做查询：用标题的正常内容（首文件名）测不出任何东西——它本来就在 `files_text` 里，
把索引列加回去也不会让任何断言变红。

### 决策 2：Web 端「换字段含义」也要提 `DB_VERSION`

`crates/web/src/idb.rs` 的既有认知是「**加 store** 必须同时提版本号」。这次的情况不在
那句话覆盖范围内：v4 与 v5 的 `inbox` 行**逐字段相同**，变的只是 `title` 的含义
（拼好的整句 → 首文件名）。

**不能指望反序列化失败来过滤旧行**：结构相同，旧行会**成功**读回来，然后被前端再拼一次
后缀，显示成「a.pdf 等 3 个文件 等 3 个文件」——无 warn 无报错，比读失败难查得多。
而 `onupgradeneeded` 是唯一知道 `old_version`、因而唯一能丢掉旧行的地方。

**丢弃的判据收得很窄**：`old_version == 0`（新库首建）不丢，拿不到
`IdbVersionChangeEvent` 也不丢。多留一批脏行只是显示难看，误删是真丢用户数据。

> 这与 `CLAUDE.md` 的「Web 端 schema 变更直接换，不写迁移 / 回填 / 双写」不冲突——
> 那条说的是不要写迁移代码，不是说不用管旧行。

### 决策 3：把「首文件」的顺序契约显式化

`inbox_content_hash` 逐文件累加 `relative_path ‖ 0x00 ‖ checksum ‖ size_le`，**顺序是
跨端去重的字节级契约**；`inbox_primary_file_name` 取第 0 个也依赖同一个顺序。

但 `crates/storage-sql/src/inbox.rs` 里 `TransferSession::load().with(TransferFile)` 的关系
加载**没有任何 `ORDER BY`**——整个契约一直靠 SQLite「不加排序时按 rowid 返回」这一实现
行为兜着。加一个 join、换个查询计划或换后端都会静默改掉 `content_hash`，而那是**不报错**
的一类损坏：去重从此失效，没有任何日志。

改为构造 facts 前显式 `sort_unstable_by_key(|f| f.id)`。**这不改变现有哈希**（rowid 顺序
本就等于 id 升序），只是把巧合变成保证。`HasMany` 只 Deref 到 `&[_]`，所以先
`into_iter().collect()` 摊成 `Vec`——是移动不是深拷，文件行里的 `completed_chunks` 位图与
`outboard` BLOB 不会被复制一份。

### 决策 4：条目级判定不得由单个文件代表

`isImageFile` / `isVideoFile` 是**文件级**谓词，而移动端拿它们的产物做**条目级**断言
——同时喂给图标、「图片」标签与筛选器。

这个缺陷跨越了两个时代且**换了方向**：

| | `title` 的内容 | 表现 |
|---|---|---|
| 散文时代 | 「a.jpg 等 3 个文件」 | **假阴性**：按扩展名「个文件」判，恒为假，媒体图标静默退化 |
| 结构化之后 | 首文件名 | **假阳性**：「封面.jpg + 50 个 zip」被归成图片，进「图片」筛选 |

假阳性更糟——用户筛「图片」筛出一堆 zip。所以修法不是「让它拿到正确的文件名」，
而是**只对单文件条目判**：多文件条目一律走中性形态，与桌面 `ItemIcon` 的「多文件一律
归档图标」同规。判据写在「条目级 vs 文件级」上，而不是写在「title 里存的是什么」上。

## Risks / Open Questions

- **`DROP COLUMN` 的 SQLite 版本下限。** 它要 3.35（2021）才有，而本仓的迁移文档此前写着
  「SQLite 不支持删列」。迁移的 up/down 测试同时钉着这一点——捆绑版本不够新会在测试里红，
  而不是在用户升级时红。文档里那句过时断言已一并纠正。
- **Web 端丢弃旧行是不可逆的。** 存量条目直接消失而不是降级显示。依据是
  `CLAUDE.md` 的既定判据「Web 端还没有真实用户」；若该前提变化，这条要重新评估。
- **`title_is_not_indexed` 依赖哨兵词。** 如果将来有人让 `inbox_items.title` 的内容
  也进入 `files_text`（例如把标题当成一个虚拟文件名），这条测试会失效且不易察觉。
