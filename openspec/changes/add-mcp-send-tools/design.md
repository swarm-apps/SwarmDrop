## Context

SwarmDrop 的所有功能当前只能通过 Tauri IPC（前端 invoke）访问。后端已有完整的传输链路：`NetManager` 管理网络状态、`DeviceManager` 追踪设备、`TransferManager` 处理 scan → prepare → send_offer 全流程。

现在需要在不改动现有架构的前提下，新增一个 MCP Server 入口，让 AI 助手通过标准 MCP 协议操控这些能力。

关键约束：
- `NetManagerState` 是 `Mutex<Option<NetManager>>`，MCP Tool 需要获取锁访问
- 传输流程已经在 `TransferManager` 中完整实现，MCP Tool 应复用而非重写
- MCP Server 必须在 Tauri 进程内运行，通过 `AppHandle` 访问状态

## Goals / Non-Goals

**Goals:**
- 实现嵌入式 MCP Server（rmcp + axum），监听 127.0.0.1:19527
- 提供 3 个 Tool：`get_network_status`、`list_available_devices`、`send_files`
- 提供 1 个 Resource：`swarmdrop://guide`（使用指南）
- 用户可通过 Tauri Command 控制 MCP 服务启停
- AI 助手可完成"查看状态 → 选择设备 → 发送文件"完整流程

**Non-Goals:**
- 不实现接收端 Tool（AI 以发送为主）
- 不实现 MCP Apps UI（无 iframe 沙箱 UI）
- 不实现文件浏览/搜索 Tool（只接受明确路径）
- 不实现传输进度轮询 Tool（fire-and-forget）
- 不修改现有 Tauri Command 或前端代码
- 不实现 MCP 鉴权（仅本地监听）

## Decisions

### 1. MCP SDK：使用 rmcp

使用 [rmcp](https://github.com/modelcontextprotocol/rust-sdk) 官方 Rust MCP SDK。

- `#[tool]` / `#[tool_router]` 宏声明 Tool
- `#[resource]` / `#[resource_router]` 宏声明 Resource
- `transport-streamable-http-server` feature 提供 axum 集成

替代方案：手写 JSON-RPC → 维护成本高，不值得。

### 2. MCP Handler 结构：持有 AppHandle

```rust
#[derive(Clone)]
pub struct McpHandler {
    app: AppHandle,
}
```

通过 `self.app.state::<NetManagerState>()` 访问 `NetManager`，与 Tauri Command 使用完全相同的状态路径。无需额外存储或状态同步。

### 3. send_files Tool：粗粒度封装

`send_files(peer_id, file_paths)` 内部串联三步：

```
file_paths → FileSource::from_paths()
          → TransferManager::prepare()     (计算 BLAKE3 hash)
          → TransferManager::send_offer()  (发送 Offer)
          → 返回 session_id
```

这比前端的 scan → prepare → start_send 更简化——跳过 scan_sources（AI 不需要文件树预览），直接从路径构造 `EnumeratedFile` 列表。

prepare 阶段需要 `Channel<PrepareProgress>` 参数用于进度上报——MCP 场景下创建一个 no-op channel 即可（不需要上报进度给 AI）。

### 4. Server 生命周期：独立启停

MCP Server 有自己的启停命令，独立于 P2P 网络：

```
McpServerState = Mutex<Option<McpServerHandle>>

McpServerHandle {
    shutdown_tx: oneshot::Sender<()>,  // graceful shutdown
    addr: SocketAddr,
}
```

MCP Server 可以在 P2P 未启动时运行。此时 `get_network_status` 返回 `stopped`，其他 Tool 返回 `isError: true`。

### 5. 端口：前端可配置，默认 19527

`start_mcp_server` 命令接收可选的 `port` 参数（`Option<u16>`），默认 19527。端口值由前端 preferences-store 持久化（tauri-plugin-store），设置页面提供输入框。

```rust
#[tauri::command]
pub async fn start_mcp_server(app: AppHandle, port: Option<u16>) -> AppResult<McpStatus> {
    let port = port.unwrap_or(19527);
    let addr: SocketAddr = format!("127.0.0.1:{}", port).parse()?;
    // ...
}
```

Claude Desktop 配置中端口需与用户设置匹配：

```json
{ "mcpServers": { "swarmdrop": { "url": "http://localhost:19527/mcp" } } }
```

### 6. list_available_devices：已配对 ∩ 在线

复用 `DeviceManager` 现有能力。DeviceManager 已有 `paired_map`（已配对设备）和 `peers`（运行时设备），MCP Tool 做交叉过滤：只返回 `is_paired && is_connected` 的设备。

## Risks / Trade-offs

**[Risk] prepare 阶段大文件 hash 耗时** → MCP Tool 调用可能需要等待较长时间。Mitigation：AI 场景文件通常不大；如果超时，可在后续版本加异步 + 状态查询。

**[Risk] rmcp API 不稳定** → 0.x 版本可能有 breaking change。Mitigation：锁定具体版本，关注 changelog。

**[Risk] 端口冲突** → 19527 可能被其他程序占用。Mitigation：启动失败时返回明确错误，后续版本可加端口配置。

**[Trade-off] fire-and-forget 模式** → AI 无法追踪传输进度。可接受：对方在 SwarmDrop UI 上接受/拒绝，用户可以自己看。后续版本可加 `get_transfer_status` Tool。
