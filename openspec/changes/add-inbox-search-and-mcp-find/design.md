## Context

inbox 数据已落在共享 core 的 SQLite（sea-orm 2.0-rc，`sqlx-sqlite`）：`inbox_items`（标题、来源、内容种类、时间戳、`archived_at`/`deleted_at`）与 `inbox_item_files`（文件名、相对路径、本地路径、`missing`）。core 不含平台代码，桌面（Tauri）与移动（uniffi）共用同一份；移动端通过 git 依赖拉取。

MCP 侧已由 `add-mcp-send-tools` 落地：`src-tauri/src/mcp/`（rmcp + axum），`McpHandler` 持 `AppHandle`，经 `app.state::<...>()` 访问与 Tauri command 同一棵状态树，工具用 `#[tool_router]` 注册，监听 `127.0.0.1:19527`。本次的"找文件"工具直接挂进这个框架。

约束：不引入云服务、不引入向量/模型依赖（保持"无云/E2E/无遥测"定位与零额外 crate）；不污染共享 core 的平台无关性。

## Goals / Non-Goals

**Goals:**
- 共享 core 提供一个本地、平台无关的 inbox 全文检索 API（`search_inbox`），桌面 UI 与 MCP 复用同一实现。
- 在已有 MCP 框架上新增 `search_inbox` / `get_inbox_file`，让 Agent 能"找文件 + 取路径"，与既有"发文件"闭环。
- 检索基于现有元数据即可工作，零新依赖、可立即对存量数据生效。

**Non-Goals:**
- OCR / 截图与文档文本抽取（schema 预留 `extracted_text`，本期不填充）。
- sqlite-vec 向量与端侧 embedding 语义检索。
- 跨设备检索（从桌面搜手机 inbox）。
- 移动端搜索 UI（仅保证 core 迁移随 git 依赖生效）。

## Decisions

### 决策 1：用 SQLite FTS5 外部内容表（external content），而非独立全文表或应用层过滤
- **选择**：建一个 FTS5 虚拟表 `inbox_fts`，以 `inbox_items` 为 `content` 外部内容源，并用触发器把 `inbox_item_files` 聚合文本（文件名、相对路径用空格拼接）写入对应行。检索时 join 回 `inbox_items` 取展示字段并应用 `deleted_at`/`archived_at` 过滤。
- **理由**：FTS5 由 bundled SQLite 自带，零新依赖；rank 排序、snippet 高亮开箱即用；external content 模式避免文本重复存储。
- **备选**：(a) 应用层 `LIKE '%kw%'` 扫描——无法分词、无 rank、数据量大时慢；(b) 独立非外部内容 FTS 表——文本被复制一份，同步成本更高。均不如外部内容表。

### 决策 2：CJK 分词用 trigram tokenizer
- **选择**：`inbox_fts` 采用 `tokenize = 'trigram'`（SQLite ≥ 3.34 内置）。
- **理由**：unicode61 默认按空白/标点切分，对中文整段不分词会导致"合同"这类子串失配；trigram 对中文、子串匹配、文件名片段都友好，且无需外部分词器，跨平台一致（移动端 bundled SQLite 同样可用）。
- **代价/备选**：trigram 对 < 3 字的查询召回受限、索引体积略大。inbox 文本量级很小，体积可接受；短查询问题在结果实测后再决定是否叠加 unicode61 列。备选 icu tokenizer 需额外编译且移动端不一定可用，放弃。

### 决策 3：检索以 item 为粒度，文件名聚合进同一索引行
- **选择**：一个 inbox_item 一条 FTS 行；该 item 下所有文件名/相对路径聚合进同一行的一个列。命中返回 item + 匹配片段，再由 `get_inbox_file` 下钻到具体文件。
- **理由**：用户/Agent 的心智是"找那一次收到的东西"（一次传输 = 一个 item），而非孤立文件；item 粒度结果更紧凑，避免同一传输的多文件刷屏。
- **备选**：file 粒度索引——结果更细但冗余且需要去重展示，留待语义检索阶段再评估。

### 决策 4：触发器维护 FTS，迁移负责一次性回填
- **选择**：在 `crates/migration` 新增一条迁移：建 `inbox_fts` + 建立 `inbox_items`/`inbox_item_files` 的 AFTER INSERT/UPDATE/DELETE 触发器；迁移末尾执行一次全量 rebuild 回填存量。
- **理由**：写路径无需改动业务代码（core 现有写入自动触发同步），关注点集中在 schema；回填保证升级用户的历史 inbox 立即可搜。
- **代价**：触发器把"文件行变化 → 重算所属 item 聚合文本"写进 SQL，逻辑需仔细写（按 `inbox_item_id` 重新聚合）。在迁移与单测里覆盖。

### 决策 5：MCP 工具复用 Tauri/core 检索接口，不直连 DB
- **选择**：`search_inbox` / `get_inbox_file` 两个 Tool 经 `McpHandler` 的 `AppHandle` 调用桌面端 `search_inbox` 能力 / 读 inbox 仓储，与前端走同一路径。
- **理由**：与 `add-mcp-send-tools` 既定的"Handler 通过 AppHandle 访问托管状态、不引入额外存储"约束一致；单一检索实现，避免 MCP 与 UI 行为漂移。

## Risks / Trade-offs

- **trigram 短查询召回不足**（< 3 字符匹配弱）→ inbox 文本少、以文件名/标题为主，先上线观察；必要时补一个 unicode61 列做并联。
- **触发器聚合逻辑写错导致索引与库不一致** → 迁移内做一次 rebuild 兜底；加针对 INSERT/UPDATE/DELETE 三路径的 core 单测，断言"改完即可搜/删完搜不到"。
- **移动端随 git 依赖吃到迁移但无 UI** → 迁移必须平台无关且幂等；本期不在 RN 暴露任何入口，确保静默生效、不影响现有功能。
- **`get_inbox_file` 暴露本地路径给 Agent** → 仅限 `127.0.0.1` 的本地 MCP（沿用现有监听约束），且 `missing` 文件明确报缺失，不返回可误用路径。
- **FTS 增加写放大** → inbox 写入频率低（每次传输一次），影响可忽略。

## Migration Plan

1. `crates/migration` 新增迁移：创建 `inbox_fts`（trigram，含预留 `extracted_text` 空列）+ 三类触发器，末尾 rebuild 回填存量。
2. `crates/core` 实现 `search_inbox(query, limit)`：FTS MATCH 查询 → join `inbox_items` 过滤 `deleted_at`/`archived_at` → 组装 `InboxSearchHit`（含 snippet）。
3. 桌面端新增 `search_inbox` Tauri command，注册进 `invoke_handler`。
4. `src-tauri/src/mcp/tools.rs` 新增 `search_inbox` / `get_inbox_file` 两个 Tool；扩展 `swarmdrop://guide`。
5. 验证：core 单测（同步/过滤/排序）+ 迁移回填测试 + 用 MCP 客户端实测两个 Tool。
- **回滚**：迁移可逆——drop 触发器与 `inbox_fts` 即可，业务表不受影响。

## Open Questions

- trigram 单列是否足够，还是首发就并联 unicode61 列？倾向先 trigram，按实测调整。
- `search_inbox` 是否需要分页/游标？首版用 `limit` 截断，inbox 量级下足够；如需要再加 offset。
- `get_inbox_file` 的文件标识用文件 id 还是 `相对路径`？倾向同时支持 id（精确）与相对路径（贴合 Agent 从 search 片段拿到的信息），实现时定。
