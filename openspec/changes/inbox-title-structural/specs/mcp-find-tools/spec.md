# mcp-find-tools

## MODIFIED Requirements

### Requirement: search_inbox MCP 工具

MCP Server SHALL 在已有的 mcp-server 框架（rmcp `#[tool_router]` 注册、`McpHandler` 经 `AppHandle` 访问托管状态）上新增一个 `search_inbox` Tool。该 Tool SHALL 接收 `query: String` 和可选 `limit: Option<u32>`（默认值由实现给定，如 20），内部调用桌面端 `search_inbox` 能力，并把结果转换为对 AI 友好的结构化输出：每个命中至少包含条目 id、**首文件名与文件数**、来源设备名、接收时间、文件列表（含文件名与相对路径，供 `get_inbox_file` 下钻）、匹配片段。

命中 SHALL NOT 包含预拼接的展示标题：该 Tool 的消费者是 agent 而不是 UI，结构化字段既让它自行组织语言，也不必被某个 locale 的散文绑住。Tool 的 `description` SHALL 与实际返回的字段保持一致——agent 按描述取字段，描述与实现漂移会让它取到未定义值。

该 Tool SHALL NOT 直接读取数据库，而是复用 core/Tauri 的检索接口。

#### Scenario: Agent 按关键词找文件

- **WHEN** MCP 客户端调用 `search_inbox`，`query` 为"合同"
- **THEN** 系统 SHALL 返回匹配的 inbox 条目列表（含首文件名与文件数、来源、文件名与相对路径、接收时间、匹配片段），供 Agent 据此决定下一步

#### Scenario: 描述与返回字段一致

- **WHEN** 收件箱相关 Tool 的返回结构发生字段变更
- **THEN** 对应的 Tool `description` SHALL 在同一次变更中同步更新
