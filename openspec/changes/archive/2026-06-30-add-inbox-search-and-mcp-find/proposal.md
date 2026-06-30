## Why

收件箱（inbox）已经沉淀了用户收到的所有文件，但目前只能在 UI 里按时间倒序翻列表，没有任何检索手段——东西越多越难找。同时 `add-mcp-send-tools` 已经让 AI Agent 能"发文件"，却还不能"找文件"：Agent 无法回答"上周老王发我的那份合同在哪"。这两个缺口本质是同一个：缺一个 inbox 检索能力。补上它，UI 和 MCP 一次都受益，且完全本地、不破坏"无云/E2E"定位。

## What Changes

- 在共享 Rust core（`crates/core`）新增 **inbox 检索**：建一张 **standalone FTS5（trigram）** 虚拟表 `inbox_fts` 索引 inbox 内容（条目标题、来源设备名、文件名、相对路径），在收件箱写入路径 inline 维护（不用触发器，详见 design 决策 4）。
- 新增 core API `search_inbox(query, limit, include_archived) -> Vec<InboxSearchHit>`：以 item 为粒度、统一走子串匹配（`LIKE`，≥3 字吃 trigram 索引、<3 字小表全扫），按接收时间倒序返回，命中带匹配片段；尊重 `deleted_at` / `archived_at` 过滤。
- 桌面端新增 Tauri command `search_inbox`，把该能力暴露给前端（为后续 UI 搜索框预留，不在本期做 UI）。
- 在**已有的** mcp-server 框架（`src-tauri/src/mcp/`，rmcp + axum）上新增 MCP 工具：
  - `search_inbox` — Agent 用关键词检索已收到的文件，返回命中条目摘要（标题、来源、文件列表含相对路径、接收时间、匹配片段）。
  - `get_inbox_file` — 按条目/文件定位单个已接收文件的本地路径（复用 core `get_inbox_item_detail`，让 Agent 拿到结果后能进一步处理）。
- 扩展 `swarmdrop://guide` Resource，补充"先 search 再操作"的使用顺序说明。
- FTS schema 预留 `extracted_text` 槽位（先留空），为后续 OCR/文本抽取做前向兼容，但**本期不实现抽取**。

## Capabilities

### New Capabilities
- `inbox-search`: 共享 core 的 inbox 检索能力——standalone FTS5（trigram）索引表、写入路径 inline 维护、`search_inbox` API（子串匹配 + 接收时间倒序）、桌面 Tauri command 暴露、删除/归档过滤。
- `mcp-find-tools`: 在现有 mcp-server 上新增的"找文件"MCP 工具集——`search_inbox` 与 `get_inbox_file`，以及指南 Resource 的对应补充。

### Modified Capabilities
（无。`add-mcp-send-tools` 引入的 `mcp-server` / `mcp-send-tools` 尚未归档进 `openspec/specs/`，本期不修改其已声明的需求，仅在同一框架上注册新工具；对它的依赖见 Impact。）

## Impact

- **依赖前置**：依赖 `add-mcp-send-tools`（mcp-server 框架、`McpHandler`/`AppHandle` 状态访问、rmcp `#[tool_router]` 注册模式）先落地。本期复用其框架，不新增 MCP server 基础设施。
- **共享 core**：`crates/core` 新增 `database` 内的检索查询逻辑，并在收件箱写入路径 inline 维护 FTS；`crates/migration` 新增一条迁移（创建 standalone FTS5 虚拟表 + 一次性回填存量）。**移动端 `SwarmDrop-RN` 通过 git 依赖共享同一份 core，迁移随之生效**，但本期不在 RN 侧加搜索 UI。
- **后端代码**：`src-tauri/src/mcp/tools.rs` 新增 2 个 Tool；`src-tauri` 新增 `search_inbox` Tauri command 并注册到 `invoke_handler`。
- **Rust 依赖**：无新增 crate（FTS5 由 bundled SQLite 自带——已确认 `libsqlite3-sys` bundled 编译带 `-DSQLITE_ENABLE_FTS5`；不引入向量/模型依赖）。
- **数据库迁移**：standalone FTS5 虚拟表为新增对象，向后兼容；首次升级时用一次性 `INSERT … SELECT` 回填存量 inbox（幂等）。
- **明确不在本期范围（Non-goals）**：OCR / 图片截图文本抽取；语义 / 向量检索（sqlite-vec 或 libSQL/turso 原生向量——均已评估，结论见 design「未来语义检索路线」：本期零理由切引擎、未来更倾向留在 SQLite）；跨设备检索（从桌面搜手机 inbox）；移动端搜索 UI。FTS schema 的 `extracted_text` 空列已为未来文本抽取预留扩展位。
