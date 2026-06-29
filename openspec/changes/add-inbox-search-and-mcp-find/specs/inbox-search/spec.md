## ADDED Requirements

### Requirement: inbox 全文索引

共享 core SHALL 在 SQLite 中维护一个对 inbox 内容的 FTS5 全文索引。索引 SHALL 覆盖条目标题（`inbox_items.title`）、来源设备名（`inbox_items.source_name`）、以及该条目下所有文件的文件名（`inbox_item_files.name`）与相对路径（`inbox_item_files.relative_path`）。索引 SHALL 通过数据库触发器在 `inbox_items` 与 `inbox_item_files` 发生 INSERT / UPDATE / DELETE 时自动保持同步，调用方无需手动维护。

#### Scenario: 新收到的条目进入索引

- **WHEN** 一次传输完成、新的 `inbox_items` 及其 `inbox_item_files` 写入数据库
- **THEN** 系统 SHALL 自动把该条目的标题、来源名、文件名、相对路径写入 FTS 索引，使其立即可被检索

#### Scenario: 条目文件变化时索引同步

- **WHEN** 某 `inbox_item_files` 行被更新或删除
- **THEN** 系统 SHALL 通过触发器同步更新 FTS 索引，使索引内容与当前数据库一致

#### Scenario: 中文与多语关键词分词

- **WHEN** 检索包含中文或混合语言关键词
- **THEN** FTS 配置 SHALL 能对中文内容产生可用的匹配（采用对 CJK 友好的 tokenizer，如 unicode61 配合按字切分或 trigram），不得因纯按空格分词而对中文整体失配

### Requirement: search_inbox 查询 API

共享 core SHALL 暴露 `search_inbox(query: &str, limit: usize) -> Vec<InboxSearchHit>` 查询接口。结果 SHALL 以 inbox 条目（item）为粒度，按相关度（FTS rank）降序排序，并截断到 `limit`。每个 `InboxSearchHit` SHALL 至少包含：条目 id、标题、来源设备名、接收时间、文件数、根路径，以及命中所在的字段或匹配片段（snippet）。查询 SHALL 排除 `deleted_at` 非空的条目；默认 SHALL 排除 `archived_at` 非空的条目，除非调用方显式要求包含已归档项。

#### Scenario: 关键词命中标题或文件名

- **WHEN** 调用 `search_inbox("合同", 20)` 且存在标题或文件名包含"合同"的未删除条目
- **THEN** 系统 SHALL 返回这些条目，按相关度排序，每个结果带匹配片段，总数不超过 20

#### Scenario: 空查询或无命中

- **WHEN** 查询为空字符串，或没有任何条目匹配
- **THEN** 系统 SHALL 返回空列表，且不报错

#### Scenario: 已删除条目不出现在结果

- **WHEN** 某条目 `deleted_at` 已被置值
- **THEN** 即使其文本匹配查询，系统 SHALL NOT 在结果中返回该条目

#### Scenario: 默认排除已归档项

- **WHEN** 调用方未要求包含已归档项，且某命中条目 `archived_at` 非空
- **THEN** 系统 SHALL NOT 默认返回该条目

### Requirement: 桌面端 search_inbox 命令

桌面端 SHALL 提供一个 Tauri command `search_inbox`，转发到 core 的 `search_inbox` 接口，把检索能力暴露给前端与 MCP 层复用。命令 SHALL 复用与其它 Tauri command 相同的托管数据库连接，不另开连接。

#### Scenario: 前端调用检索

- **WHEN** 前端以查询词调用 `search_inbox` 命令
- **THEN** 系统 SHALL 返回与 core `search_inbox` 一致的结果列表

#### Scenario: 数据库未就绪

- **WHEN** 数据库连接尚未初始化时调用该命令
- **THEN** 系统 SHALL 返回错误说明数据库未就绪，而不是 panic

### Requirement: 存量索引回填

引入 FTS 索引的迁移 SHALL 在首次升级时对已存在的 inbox 数据做一次性回填（rebuild），使升级前收到的历史条目同样可被检索。回填 SHALL 是幂等的：重复执行不产生重复索引行。

#### Scenario: 升级后历史条目可搜

- **WHEN** 用户从无索引版本升级，库中已有历史 inbox 条目
- **THEN** 迁移完成后这些历史条目 SHALL 可通过 `search_inbox` 检索到

### Requirement: FTS schema 前向兼容文本抽取

FTS 索引 schema SHALL 预留一个 `extracted_text` 文本列用于未来承载 OCR / 文档文本抽取的结果，但本能力 SHALL NOT 在本期填充该列。当该列为空时，检索行为 SHALL 与不存在该列时一致，不得因空列影响匹配或排序。

#### Scenario: extracted_text 为空时检索正常

- **WHEN** 所有条目的 `extracted_text` 均为空
- **THEN** `search_inbox` SHALL 仅基于标题、来源名、文件名、相对路径正常返回结果
