## 1. 依赖与模块脚手架

- [x] 1.1 在 `src-tauri/Cargo.toml` 添加 `rmcp`（含 `server` + `transport-streamable-http-server` feature）和 `axum` 依赖
- [x] 1.2 创建 `src-tauri/src/mcp/` 模块结构：`mod.rs`、`server.rs`、`tools.rs`、`resources.rs`
- [x] 1.3 在 `src-tauri/src/lib.rs` 中声明 `mod mcp` 模块

## 2. MCP Server 基础架构

- [x] 2.1 在 `mcp/mod.rs` 定义 `McpHandler` 结构（持有 `AppHandle`），实现 `#[tool_router]` 和 `#[resource_router]`
- [x] 2.2 在 `mcp/server.rs` 实现 `start()` 函数：创建 axum HTTP Server，绑定 `127.0.0.1:19527`，支持 graceful shutdown
- [x] 2.3 定义 `McpServerHandle`（含 `shutdown_tx: oneshot::Sender<()>` 和 `addr: SocketAddr`）
- [x] 2.4 定义 `McpServerState` 类型别名（`Mutex<Option<McpServerHandle>>`）

## 3. Tauri 启停命令

- [x] 3.1 实现 `start_mcp_server` Tauri Command：接收可选 `port: Option<u16>`（默认 19527），启动 MCP Server，存入 `McpServerState`，返回 `McpStatus`
- [x] 3.2 实现 `stop_mcp_server` Tauri Command：发送 shutdown signal，清理 `McpServerState`
- [x] 3.3 在 `lib.rs` 的 `invoke_handler` 中注册这两个命令

## 4. MCP Tool 实现

- [x] 4.1 实现 `get_network_status` Tool：从 `NetManagerState` 读取网络状态，节点未启动时返回 `{ status: "stopped" }`
- [x] 4.2 实现 `list_available_devices` Tool：从 `DeviceManager` 获取已配对且在线的设备列表（`is_paired && is_connected`）
- [x] 4.3 实现 `send_files` Tool：接收 `peer_id` + `file_paths`，内部串联文件枚举 → `TransferManager::prepare()` → `TransferManager::send_offer()`，返回 session_id
- [x] 4.4 实现 `AppError` → MCP 错误的转换辅助函数（`isError: true` + 错误消息）

## 5. MCP Resource 实现

- [x] 5.1 创建 `src-tauri/docs/mcp-guide.md` 使用指南文档（含前置条件、Tool 使用顺序、典型流程）
- [x] 5.2 在 `mcp/resources.rs` 实现 `swarmdrop://guide` Resource，使用 `include_str!` 嵌入文档

## 6. 前端命令封装

- [x] 6.1 创建 `src/commands/mcp.ts`：封装 `startMcpServer()` / `stopMcpServer()` TypeScript 函数

## 7. 验证

- [x] 7.1 `cargo build` 编译通过
- [ ] 7.2 启动 MCP Server，用 curl 测试 `tools/list` 端点确认 3 个 Tool 和 1 个 Resource 已注册
- [ ] 7.3 用 curl 测试 `get_network_status` Tool 调用
