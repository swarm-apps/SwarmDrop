## MODIFIED Requirements

### Requirement: search_inbox MCP 工具

MCP Server SHALL 在已有的 mcp-server 框架（rmcp `#[tool_router]` 注册、`McpHandler` 经 `AppHandle` 访问托管状态）上提供一个 `search_inbox` Tool。该 Tool SHALL 接收 `query: String`、可选 `limit: Option<u32>`（默认值由实现给定，如 20），以及可选 `include_archived: Option<bool>`（默认 `false`）。`include_archived` SHALL 透传给 core `search_inbox(db, query, limit, include_archived)`：为 `false` 时排除已归档条目，为 `true` 时纳入已归档条目。该 Tool SHALL NOT 直接读取数据库，而是复用 core / Tauri 的检索接口，并把结果转换为对 AI 友好的结构化输出：每个命中至少包含条目 id、标题、来源设备名、接收时间、文件数与文件列表（含文件名与相对路径，供 `get_inbox_file` 下钻）、匹配片段。该 Tool SHALL 标注 `readOnlyHint`。

#### Scenario: Agent 按关键词找文件

- **WHEN** MCP 客户端调用 `search_inbox`，`query` 为"合同"
- **THEN** 系统 SHALL 返回匹配的 inbox 条目列表（含标题、来源、文件名与相对路径、接收时间、匹配片段），供 Agent 据此决定下一步

#### Scenario: 包含已归档条目

- **WHEN** MCP 客户端调用 `search_inbox` 且 `include_archived` 为 `true`
- **THEN** 检索结果 SHALL 纳入已归档（`archived_at` 非空）的命中条目；为 `false` 或缺省时 SHALL 排除已归档条目

#### Scenario: 无命中

- **WHEN** `query` 没有匹配任何条目
- **THEN** 系统 SHALL 返回空结果集且不报错，输出可读地说明"未找到匹配项"

#### Scenario: 数据库未就绪

- **WHEN** 调用时数据库连接尚未初始化
- **THEN** Tool SHALL 返回 `isError: true`，消息说明检索暂不可用，而不是使服务崩溃
