## ADDED Requirements

### Requirement: MCP Server 基础架构

系统 SHALL 提供一个嵌入式 MCP Server，基于 rmcp SDK 和 axum HTTP 框架，在 Tauri 进程内运行。Server SHALL 监听 `127.0.0.1:<port>`，端口由前端传入（默认 19527），仅接受本地连接。Server SHALL 实现 MCP Streamable HTTP 传输协议，兼容 Claude Desktop 等 MCP 客户端。

#### Scenario: MCP Server 成功启动（默认端口）

- **WHEN** 用户调用 `start_mcp_server` 未指定端口
- **THEN** 系统 SHALL 在 `127.0.0.1:19527` 启动 HTTP 服务，返回 `{ running: true, addr: "127.0.0.1:19527" }`

#### Scenario: MCP Server 成功启动（自定义端口）

- **WHEN** 用户调用 `start_mcp_server` 指定 port 为 8080
- **THEN** 系统 SHALL 在 `127.0.0.1:8080` 启动 HTTP 服务，返回 `{ running: true, addr: "127.0.0.1:8080" }`

#### Scenario: MCP Server 重复启动

- **WHEN** MCP Server 已在运行，用户再次调用 `start_mcp_server`
- **THEN** 系统 SHALL 返回当前状态而不创建新实例

#### Scenario: MCP Server 停止

- **WHEN** 用户调用 `stop_mcp_server` 命令
- **THEN** 系统 SHALL 发送 shutdown signal 并优雅关闭 HTTP 服务

#### Scenario: 端口被占用

- **WHEN** 19527 端口已被其他程序占用
- **THEN** 系统 SHALL 返回错误信息说明端口冲突

### Requirement: MCP Handler 状态访问

MCP Handler SHALL 通过 `AppHandle` 访问 Tauri 托管状态（`NetManagerState`、`Keypair` 等），与 Tauri Command 共享同一个状态树。SHALL 不引入额外的状态存储。

#### Scenario: 访问网络状态

- **WHEN** MCP Tool 需要读取网络状态
- **THEN** Handler SHALL 通过 `app.state::<NetManagerState>()` 获取 `NetManager`，调用其现有方法

#### Scenario: P2P 节点未启动

- **WHEN** MCP Tool 访问 `NetManagerState` 时内部值为 `None`
- **THEN** Tool SHALL 返回 `isError: true`，消息为 "P2P 网络节点未启动"

### Requirement: MCP Server 状态管理

系统 SHALL 使用 `McpServerState`（`Mutex<Option<McpServerHandle>>`）管理 Server 生命周期。`McpServerHandle` SHALL 包含 shutdown 信号发送端和监听地址。

#### Scenario: 状态存储

- **WHEN** MCP Server 启动成功
- **THEN** 系统 SHALL 将 `McpServerHandle` 存入 `McpServerState`

#### Scenario: 状态清理

- **WHEN** MCP Server 停止
- **THEN** 系统 SHALL 将 `McpServerState` 内部值设为 `None`

### Requirement: 使用指南 Resource

MCP Server SHALL 提供一个 Resource，URI 为 `swarmdrop://guide`，MIME 类型为 `text/markdown`，包含 SwarmDrop MCP 使用指南。指南 SHALL 说明前置条件、Tool 使用顺序、配对和发送流程。

#### Scenario: 读取使用指南

- **WHEN** MCP 客户端请求 `swarmdrop://guide` Resource
- **THEN** 系统 SHALL 返回 Markdown 格式的使用指南文档
