# inbox-search

## MODIFIED Requirements

### Requirement: inbox 检索索引

共享 core SHALL 在 SQLite 中维护一张以 inbox 条目（item）为粒度的预聚合文本表，索引 inbox 内容。索引 SHALL 覆盖来源设备名（`inbox_items.source_name`）、该条目下所有文件的文件名（`inbox_item_files.name`）与相对路径（`inbox_item_files.relative_path`），以及文档正文抽取结果（`extracted_text`，仅具备抽取能力的平台）。

索引 SHALL NOT 收录**可由其他已索引字段完全派生**的展示字段。判据是该字段相对于已索引内容有无**独立信息**：条目的展示标题取自首个文件名，而文件名已在文件文本列中（该列以 `"{name} {relative_path}"` 逐文件拼接，必然以首文件名开头），故它 SHALL NOT 作为独立索引列存在。

索引 SHALL 在收件箱写入条目时维护、并由迁移对存量数据一次性回填；维护机制对调用方透明，调用方无需手动维护。索引内容 SHALL 与当前 inbox 保持一致。

#### Scenario: 新收到的条目进入索引

- **WHEN** 一次传输完成、新的 `inbox_items` 及其 `inbox_item_files` 写入数据库
- **THEN** 系统 SHALL 在同一写入路径把该条目的来源名、文件名、相对路径写入索引，使其立即可被检索

#### Scenario: 索引与收件箱内容保持一致

- **WHEN** 检索任一已存在条目（无论是本版本新写入的，还是升级前由回填导入的存量条目）
- **THEN** 检索结果 SHALL 与当前 inbox 内容一致，调用方无需手动重建或维护索引

#### Scenario: 中文与两字词检索

- **WHEN** 检索包含中文关键词，特别是 2 个汉字的常见词（如"合同""发票"）
- **THEN** 系统 SHALL 能对中文内容产生匹配；实现 SHALL 通过子串匹配兜底少于 3 个字符的查询，不得因 trigram 的 3-gram 下限或纯空格分词而对中文短词整体失配

#### Scenario: 展示标题不作为独立索引列

- **WHEN** 检索一个只出现在条目展示标题字段、而不出现在来源名 / 文件名 / 相对路径 / 抽取正文中的词
- **THEN** 系统 SHALL NOT 返回任何条目；命中 SHALL 仅由来源名、文件名、相对路径或抽取正文产生

> 这条只能用「只存在于标题字段」的词来验证。标题的正常内容（首文件名）本就在文件文本列里，
> 拿它做查询词，加不加索引列都会命中。

### Requirement: search_inbox 查询 API

共享 core SHALL 暴露 `search_inbox(query, limit, include_archived) -> Vec<InboxSearchHit>` 查询接口。结果 SHALL 以 inbox 条目（item）为粒度，按接收时间（`received_at`）倒序排序，并截断到 `limit`。检索 SHALL 采用子串匹配（对索引文本列做 `LIKE`，≥3 个字符的查询可经 trigram 索引加速、更短的查询退化为全表扫描但结果正确），不依赖 FTS bm25 排序。每个 `InboxSearchHit` SHALL 至少包含：条目 id、**首文件名与文件数**（供调用方按当前 locale 生成展示标题）、来源设备名、接收时间、根路径，以及命中所在字段的匹配片段（snippet，由实现生成）。`InboxSearchHit` SHALL NOT 包含预拼接的展示标题。查询 SHALL 排除 `deleted_at` 非空的条目；默认 SHALL 排除 `archived_at` 非空的条目，除非 `include_archived` 显式要求包含已归档项。

#### Scenario: 两字中文词命中文件名

- **WHEN** 调用 `search_inbox("合同", 20, false)` 且存在文件名包含"合同"的未删除条目
- **THEN** 系统 SHALL 返回这些条目（不得因"合同"仅 2 个字而返回空），按接收时间倒序，每个结果带匹配片段，总数不超过 20

#### Scenario: 空查询或无命中

- **WHEN** 查询为空字符串，或没有任何条目匹配
- **THEN** 系统 SHALL 返回空列表，且不报错

#### Scenario: 命中已在条目行可见的内容时不产生片段

- **WHEN** 查询命中的是来源设备名或首文件名（两者在条目行上已直接显示）
- **THEN** 匹配片段 SHALL 为空值，调用方 SHALL NOT 渲染重复的片段行

#### Scenario: 命中非首文件时产生片段

- **WHEN** 多文件条目中，查询命中的是首文件以外的某个文件名或相对路径
- **THEN** 系统 SHALL 返回该命中位置的片段

#### Scenario: 已删除条目不出现在结果

- **WHEN** 某条目 `deleted_at` 已被置值
- **THEN** 即使其文本匹配查询，系统 SHALL NOT 在结果中返回该条目

#### Scenario: 默认排除已归档项

- **WHEN** `include_archived` 为 false，且某命中条目 `archived_at` 非空
- **THEN** 系统 SHALL NOT 返回该条目

### Requirement: FTS schema 前向兼容文本抽取

索引 schema SHALL 预留一个 `extracted_text` 文本列用于未来承载 OCR / 文档文本抽取的结果，但本能力 SHALL NOT 在本期填充该列。当该列为空时，检索行为 SHALL 与不存在该列时一致，不得因空列影响匹配或排序。

> Requirement 标题里的「FTS」是历史称谓：该表 2026-08-05 起已是普通表。标题**刻意不改**——
> 改标题会让同步后的主 spec 里新旧两条并存（delta 的同步规则是「保留未提及的内容」），
> 而重命名不值得为此单开一次 REMOVED + ADDED。

#### Scenario: extracted_text 为空时检索正常

- **WHEN** 所有条目的 `extracted_text` 均为空
- **THEN** `search_inbox` SHALL 仅基于来源名、文件名、相对路径正常返回结果
