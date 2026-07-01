## Context

inbox 数据已落在共享 core 的 SQLite（sea-orm 2.0-rc，`sqlx-sqlite`）：`inbox_items`（主键是 **UUID**，存为 `uuid_text` TEXT；标题、来源、内容种类、时间戳、`archived_at`/`deleted_at`）与 `inbox_item_files`（文件名、相对路径、本地路径、`missing`）。core 不含平台代码，桌面（Tauri）与移动（uniffi）共用同一份；移动端通过 git 依赖拉取。已确认 bundled SQLite 带 FTS5（`libsqlite3-sys` 的 bundled 编译含 `-DSQLITE_ENABLE_FTS5`，版本 ≈3.50，trigram tokenizer 可用），且 `sqlx-sqlite` 默认开 `foreign_keys`。

MCP 侧已由 `add-mcp-send-tools` 落地：`src-tauri/src/mcp/`（rmcp + axum），`McpHandler` 持 `AppHandle`，经 `app.state::<...>()` 访问与 Tauri command 同一棵状态树（含 `app.manage(db)` 注入的裸 `DatabaseConnection`），工具用 `#[tool_router]` 注册，监听 `127.0.0.1:19527`。本次的"找文件"工具直接挂进这个框架。

约束：不引入云服务、不引入向量/模型依赖（保持"无云/E2E/无遥测"定位与零额外 crate）；不污染共享 core 的平台无关性。

## Goals / Non-Goals

**Goals:**
- 共享 core 提供一个本地、平台无关的 inbox 检索 API（`search_inbox`），桌面 UI 与 MCP 复用同一实现。
- 在已有 MCP 框架上新增 `search_inbox` / `get_inbox_file`，让 Agent 能"找文件 + 取路径"，与既有"发文件"闭环。
- 检索基于现有元数据即可工作，零新依赖、可立即对存量数据生效。

**Non-Goals:**
- OCR / 截图与文档文本抽取（schema 预留 `extracted_text`，本期不填充）。
- 语义 / 向量检索（sqlite-vec 或 libSQL/turso 原生向量；均已评估，结论见末节「未来语义检索路线」）。
- 跨设备检索（从桌面搜手机 inbox）。
- 移动端搜索 UI（仅保证 core 迁移随 git 依赖生效）。

## Decisions

### 决策 1：用 standalone（内部内容）FTS5 表，而非 external content
- **选择**：建一张 standalone FTS5 虚拟表 `inbox_fts(item_id UNINDEXED, title, source_name, files_text, extracted_text, tokenize='trigram')`，文本由收件箱写入路径维护（见决策 4）。检索时按 `item_id` join 回 `inbox_items` 取展示字段并过滤 `deleted_at`/`archived_at`。
- **理由（为什么不是 external content）**：`inbox_items` 主键是 UUID（`uuid_text` TEXT，**没有可用的 INTEGER `content_rowid`**），且要索引的 `files_text` 来自**另一张表** `inbox_item_files` 的聚合、`extracted_text` 是预留空列——两者都不是 `inbox_items` 的列。而 external content 在 DELETE/UPDATE/rebuild 时必须按 `content_rowid` 从 content 表回读**全部被索引列**（`SELECT <rowid>, <cols> FROM <content> WHERE <rowid>=?`，见 SQLite FTS5 文档 4.4.4），这个回读契约在本 schema 下成立不了，会产出不一致索引。**所以 external content 不是"省不省空间"的取舍，而是不可行。**
- **备选/不取**：(a) contentless（`content=''`）/`contentless_delete` 更差——不存原文，`snippet()`/`highlight()` 无法重建片段，且非 MATCH 的 `LIKE` 子串查询受限；(b) 不建 FTS 表、纯应用层 `LIKE` 扫描——当前文本量级下可行，但仍建 FTS5 表是为 ≥3 字查询走 trigram 索引、并为 `extracted_text` 未来塞入大段 OCR 文本时保留索引护城河。standalone 多存一份文本，但 inbox 文本量级极小（每条几百字节），代价可忽略。

### 决策 2：trigram tokenizer + 单路 `LIKE` 子串检索（放弃 bm25 rank）
- **选择**：`tokenize='trigram'`；`search_inbox` 统一走 `文本列 LIKE '%kw%'` 子串匹配，**不用 `MATCH`/bm25**。结果按 `received_at` 倒序，匹配片段在 Rust 端按子串位置切窗口生成。
- **理由**：
  - trigram 把文本切成 3 字符 gram，**查询串少于 3 个 unicode 字符切不出 token，`MATCH` 直接命中不了**——而 2 字中文词（"合同""发票"）正是中文检索最高频形态，是招牌场景。纯 `MATCH` 会让用户搜"合同"得到 0 条，是真 bug，不能用"把验收例子改成 ≥3 字"蒙混。
  - trigram 索引能加速 FTS 列上的 `LIKE`/`GLOB`，但仅当模式含 ≥3 个连续非通配字符；恰好 2 字的 `%合同%` 用不了索引、退化为全表扫描——**结果正确，靠 inbox 量级小兜底（每次传输一条、文本短，亚毫秒级）**。
  - 统一走 `LIKE`：一套代码覆盖任意查询长度（≥3 字吃 trigram 索引、<3 字小表全扫），排序与片段天然一致，消除 `MATCH`/`LIKE` 两路分叉以及"两路 snippet/排序不一致"的坑。
  - 放弃 bm25 的代价：在"个人收件箱、命中通常个位数"的量级，接收时间倒序比 bm25 相关度更符直觉，bm25 价值很低。
- **备选/不取**：保留 `MATCH`+bm25 两路（≥3 走 MATCH、<3 降级 LIKE）——能拿 bm25 但要维护两套排序/snippet 契约，复杂度不值；`better-trigram` 等自定义 tokenizer——会丢 `LIKE`/`GLOB` 支持且是非 bundled 外部扩展，违背零依赖，放弃。

### 决策 3：检索以 item 为粒度，文件名聚合进同一索引行
- **选择**：一个 inbox_item 一条 FTS 行；该 item 下所有文件名 / 相对路径空格拼接聚合进 `files_text` 一列。命中返回 item + 匹配片段，再由 `get_inbox_file` 下钻到具体文件。
- **理由**：用户/Agent 的心智是"找那一次收到的东西"（一次传输 = 一个 item），而非孤立文件；item 粒度结果更紧凑，避免同一传输的多文件刷屏。
- **备选**：file 粒度索引——结果更细但冗余且需去重展示，留待语义检索阶段再评估。

### 决策 4：FTS 在收件箱写入路径 inline 维护，迁移显式回填存量（不用触发器）
- **选择**：在 core `ensure_inbox_item_for_completed_receive_session` 的事务内、文件循环之后，写一条聚合好的 FTS 行（`INSERT INTO inbox_fts(item_id,title,source_name,files_text,extracted_text) VALUES(...)`，`files_text` = 该 item 所有文件名 + 相对路径空格拼接，`extracted_text` 留空）。迁移只负责建表 + 一次性显式回填存量：`INSERT INTO inbox_fts SELECT ... FROM inbox_items JOIN inbox_item_files GROUP BY inbox_item_id`，与 inline 写入复用同一段聚合逻辑。
- **理由（为什么不用触发器）**：被索引的四字段（`title` / `source_name` / 文件名 / 相对路径）在 item 落库后**写一次不再变**——所有 mutation（`mark_inbox_item_opened` / `archive_inbox_item` / `delete_inbox_item_record` / `mark_inbox_item_file_missing`）只动 `last_opened_at`/`archived_at`/`deleted_at`/`missing` 等**非索引列**，删除是软删（`UPDATE deleted_at`），全代码库无 inbox 硬删（`clear_all_history` 明确保留 inbox，有测试 `clear_history_should_keep_inbox_records` 守护）。所以 UPDATE/DELETE 触发器是死代码。而 INSERT 触发器在多文件聚合上反而更绕：`ensure_inbox_item` 先插 item 再循环插多个 file 行——`inbox_items` 的 AFTER INSERT 触发时 files 还没插（聚合到空），`inbox_item_files` 的 AFTER INSERT 每行触发一次、要对同一 FTS 行 delete+重聚合（FTS5 不支持单列增量 UPSERT）。事务末尾 item + 全部 files 都在手，一次 `INSERT` 最干净，正好规避触发器的聚合痛点。
- **代价/约束**：
  - (a) 软删/归档的"搜不到"全靠**查询层 join 过滤**——`search_inbox` 每条路径都必须 join `inbox_items` 并过滤 `deleted_at`/`archived_at`（与 `list_inbox_items` 一致）。
  - (b) 将来若引入 purge 硬删或让标题/文件名可编辑，须在该写路径同步删/更 FTS（当前无此路径，暂无问题）。
  - (c) standalone 表的存量回填须显式 `INSERT ... SELECT`——FTS5 的 `'rebuild'` 命令对刚建的空 standalone 表无意义（它从已存内容重建）。

### 决策 5：MCP 工具复用 core 检索/详情接口，不直连 DB、不新增 core API
- **选择**：`search_inbox` Tool 经 `McpHandler` 的 `AppHandle` 取托管 `DatabaseConnection`、调用 core `search_inbox`；`get_inbox_file` Tool **复用已有的 `get_inbox_item_detail(db, item_id)`**（已返回带 `local_path`/`missing`/`size` 的文件列表），按文件标识（file id 或相对路径）从中挑出目标文件——**不新增 core 仓储函数**。
- **理由**：与 `add-mcp-send-tools` 既定约束一致（Handler 经 AppHandle 访问托管状态、不引额外存储）；单一检索/详情实现，避免 MCP 与 UI 漂移。

## Risks / Trade-offs

- **2 字中文查询走全表扫描**（trigram 索引仅加速 ≥3 字模式）→ 结果正确，靠 inbox 量级小兜底（每次传输一条、文本短，亚毫秒）；inbox 极大时再评估。
- **软删/归档的"搜不到"依赖查询层 join 过滤、而非索引**（因 FTS 写一次不删）→ `search_inbox` 每条路径强制 join `inbox_items` 过滤 `deleted_at`/`archived_at`；加单测断言已删/已归档不返回。
- **inline 写入要与迁移回填的聚合 SQL 保持一致** → 两处复用同一段"按 item 聚合 `files_text`"逻辑；加单测断言新增 item 立即可搜、回填后历史可搜。
- **未来若加 inbox 硬删 / 标题可编辑会让 FTS 漂移** → 当前无此路径；引入时须在写路径同步维护 FTS（决策 4 已注明）。
- **`get_inbox_file` 暴露本地路径给 Agent** → 仅限 `127.0.0.1` 的本地 MCP（沿用现有监听约束），`missing`/不可达明确报缺失，不返回可误用路径。
- **FTS 增加写放大** → inbox 写入频率低（每次传输一次），可忽略。

## Migration Plan

1. `crates/migration` 新增迁移：建 standalone `inbox_fts`（trigram，列 `item_id UNINDEXED` + `title`/`source_name`/`files_text`/`extracted_text`，`extracted_text` 留空）；末尾显式 `INSERT INTO inbox_fts SELECT ... FROM inbox_items JOIN inbox_item_files GROUP BY inbox_item_id` 回填存量（幂等：回填前确保表为空 / 可重入）；注册到 Migrator。`down` 仅 `DROP inbox_fts`。
2. `crates/core`：在 `ensure_inbox_item_for_completed_receive_session` 事务内 inline 写聚合 FTS 行；实现 `search_inbox(query, limit, include_archived)`：归一化查询 → `文本列 LIKE '%kw%'` → join `inbox_items` 过滤 `deleted_at`/`archived_at` → `received_at desc` → 组装 `InboxSearchHit`（Rust 端切 snippet）→ `limit` 截断。
3. 桌面端新增 `search_inbox` Tauri command，注册进 `invoke_handler`。
4. `src-tauri/src/mcp/tools.rs` 新增 `search_inbox`（调 core）/ `get_inbox_file`（复用 `get_inbox_item_detail`）两个 Tool；扩展 `src-tauri/docs/mcp-guide.md`。
5. 验证：core 单测（inline 写入即可搜 / 过滤 / 排序+limit / CJK 2 字 / 回填）+ 用 MCP 客户端实测两个 Tool。
- **回滚**：迁移可逆——`DROP inbox_fts` 即可，业务表不受影响。

## Open Questions

- `search_inbox` 分页：首版用 `limit` 截断，inbox 量级足够；如需要再加 `offset`。
- `get_inbox_file` 的文件标识：同时支持 file id（精确）与 `relative_path`（贴合 Agent 从 search 片段拿到的信息）；为此 `search_inbox` 的输出须回带每个文件的 `relative_path`（见 `mcp-find-tools` spec）。

## 未来语义检索路线（本期 Non-goal，已预研结论）

本期只做关键词检索，**没有任何理由现在切数据库引擎**。以下结论固化调研，供未来增量决策时直接引用，避免重复论证：

- **不采用 libSQL / turso**：libSQL 原生向量（`F32_BLOB` / `vector_distance_cos` / DiskANN，MIT、可纯本地无云）功能真实完整，但对本项目不划算——(1) libSQL 无 sqlx 支持（`sqlx#2674` 至今 open）、SeaORM 无 libsql 后端，采用 = 放弃整条 sea-orm + sqlx 栈、手写约 2200 行数据层；(2) 把整套 SQLite fork 塞进移动端 uniffi megazord，会与现有 sqlx 的 `libsqlite3-sys`（bundled SQLite）形成"双份 SQLite"链接冲突风险；(3) Turso 研发重心已转向 beta 的 **Turso Database**（Rust 重写、前身 Limbo），libSQL 在被降级、迁移路径未定。
- **未来若做语义检索，留在 SQLite 上、按需升级**：最简起点是"普通 SeaORM 表存 embedding BLOB + 在 Rust(core) 对小体量 inbox 做暴力余弦"——零 SQLite 扩展、桌面/RN(uniffi) 行为一致；仅当 N 大到 Rust 暴力扫描吃力、且 `sqlite-vec` 到 v1 + sqlx 静态链路成熟时，再升级到 `vec0` 虚拟表。`sqlite-vec` 现状：v0.1.x stable 但 pre-v1、对外只稳定暴力 KNN，且与 `sqlx-sqlite`(bundled) 集成有摩擦（sqlx 不暴露 `sqlite3_auto_extension`、依赖树仅一份 `libsqlite3-sys`、iOS 不能 `dlopen` 运行时扩展，须 `build.rs` 静态编译 + `auto_extension`）。
- **真正的瓶颈不是向量库而是端侧 embedding 生成**：无云约束下只能端侧本地模型（fastembed-rs/ort(ONNX) 或 candle，30–200MB 权重 + 每平台一份重型推理依赖、uniffi 交叉编译重）。这与选哪个向量库正交，是语义检索的可行性闸门，应作为独立预研，决策点在"是否值得为 inbox 搜索背上一个重型跨平台 AI 依赖"。
- **本期对未来友好**：`extracted_text` 空列已为文本抽取留口子，未来加向量是非破坏性增量（再 CREATE 一张表即可），无需现在为它改引擎或加依赖。
