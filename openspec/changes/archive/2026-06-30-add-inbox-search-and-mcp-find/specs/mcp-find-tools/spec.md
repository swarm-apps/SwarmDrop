## ADDED Requirements

### Requirement: search_inbox MCP 工具

MCP Server SHALL 在已有的 mcp-server 框架（rmcp `#[tool_router]` 注册、`McpHandler` 经 `AppHandle` 访问托管状态）上新增一个 `search_inbox` Tool。该 Tool SHALL 接收 `query: String` 和可选 `limit: Option<u32>`（默认值由实现给定，如 20），内部调用桌面端 `search_inbox` 能力，并把结果转换为对 AI 友好的结构化输出：每个命中至少包含条目 id、标题、来源设备名、接收时间、文件数与文件列表（含文件名与相对路径，供 `get_inbox_file` 下钻）、匹配片段。该 Tool SHALL NOT 直接读取数据库，而是复用 core/Tauri 的检索接口。

#### Scenario: Agent 按关键词找文件

- **WHEN** MCP 客户端调用 `search_inbox`，`query` 为"合同"
- **THEN** 系统 SHALL 返回匹配的 inbox 条目列表（含标题、来源、文件名与相对路径、接收时间、匹配片段），供 Agent 据此决定下一步

#### Scenario: 无命中

- **WHEN** `query` 没有匹配任何条目
- **THEN** 系统 SHALL 返回空结果集且不报错，输出可读地说明"未找到匹配项"

#### Scenario: 数据库未就绪

- **WHEN** 调用时数据库连接尚未初始化
- **THEN** Tool SHALL 返回 `isError: true`，消息说明检索暂不可用，而不是使服务崩溃

### Requirement: get_inbox_file MCP 工具

MCP Server SHALL 新增一个 `get_inbox_file` Tool，用于在检索命中后定位单个已接收文件的本地路径，便于 Agent 进一步处理。该 Tool SHALL 接收定位参数（条目 id，以及条目内文件标识，如文件 id 或相对路径），复用 core 的 inbox 详情接口（`get_inbox_item_detail`）而非直连数据库，返回该文件的 `local_path`、文件名、大小，以及 `missing` 标志。当目标文件在磁盘上已不存在（`missing = true` 或路径不可达）时，Tool SHALL 明确告知文件缺失而非返回无效路径。

#### Scenario: 定位命中文件路径

- **WHEN** Agent 用某条目 id 及文件标识调用 `get_inbox_file`，且文件存在
- **THEN** 系统 SHALL 返回该文件的本地路径、文件名、大小

#### Scenario: 文件已被外部移动或删除

- **WHEN** 目标 `inbox_item_files` 记录的 `missing` 为 true，或本地路径已不可达
- **THEN** Tool SHALL 返回标识文件缺失的结果，并保留原记录信息（文件名等），不返回可被误用的路径

#### Scenario: 条目或文件不存在

- **WHEN** 给定的条目 id 或文件标识在库中不存在
- **THEN** Tool SHALL 返回 `isError: true`，消息说明未找到对应条目/文件

### Requirement: 使用指南补充"先检索后操作"

`swarmdrop://guide` Resource SHALL 补充对新增检索工具的说明，明确推荐的工具使用顺序：先用 `search_inbox` 定位条目，再用 `get_inbox_file` 取路径，必要时配合既有的发送工具。指南 SHALL 说明检索范围仅限本机已接收的 inbox 内容，不跨设备。

#### Scenario: 客户端读取更新后的指南

- **WHEN** MCP 客户端请求 `swarmdrop://guide`
- **THEN** 返回的指南 SHALL 包含 `search_inbox` / `get_inbox_file` 的用途、参数与推荐调用顺序，并声明检索仅覆盖本机 inbox
