## Why

SwarmDrop 需要让 AI 助手（如 Claude Desktop）能通过 MCP 协议操控 P2P 文件传输。当前所有功能只能通过前端 UI 使用，AI 无法参与。第一步聚焦"AI 发送文件"这一最高频场景：查看网络状态、找到在线已配对设备、一步发送文件。

## What Changes

- 新增 `src-tauri/src/mcp/` 模块，基于 rmcp（官方 Rust MCP SDK）+ axum 实现嵌入式 MCP Server
- 新增 3 个 MCP Tool：
  - `get_network_status` — 获取 P2P 节点运行状态
  - `list_available_devices` — 列出已配对且在线的设备
  - `send_files` — 粗粒度一步发送（内部串联 scan → prepare → start_send）
- 新增 2 个 Tauri Command：`start_mcp_server` / `stop_mcp_server`，供前端控制 MCP 服务启停
- 新增 1 个 MCP Resource：`swarmdrop://guide` 使用指南
- MCP Server 仅监听 `127.0.0.1:<port>`，端口由前端配置（默认 19527），用户主动启用

## Capabilities

### New Capabilities

- `mcp-server`: MCP Server 基础架构（rmcp 集成、axum HTTP 服务、启停管理、Tool/Resource 注册框架）
- `mcp-send-tools`: MCP Tool 实现（get_network_status、list_available_devices、send_files）及使用指南 Resource

### Modified Capabilities

（无，本次不修改现有功能的需求规格）

## Impact

- **Rust 依赖**：新增 `rmcp`（含 `transport-streamable-http-server` feature）、`axum`
- **后端代码**：新增 `src-tauri/src/mcp/` 模块（mod.rs、server.rs、tools.rs、resources.rs）
- **Tauri 命令**：新增 `start_mcp_server` / `stop_mcp_server`，注册到 invoke_handler
- **前端**：新增 `src/commands/mcp.ts` 命令封装（可选，设置页开关用）
- **文档**：新增 `src-tauri/docs/mcp-guide.md`
- **端口占用**：127.0.0.1:19527
