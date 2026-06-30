## Why

SwarmDrop 的本地 MCP Server（rmcp + axum，仅 `127.0.0.1`）已落地 5 个工具（发送三件套 `get_network_status` / `list_available_devices` / `send_files` + 检索 `search_inbox` / `get_inbox_file`），让 AI Agent 能"发文件 + 找文件"。这是 SwarmDrop 在 AI 时代最大的先发优势——多数本地工具只做"读"，它已能被 Agent 驱动"跨网投递 + 检索"。

但当前 agent 驱动面有三个缺口，使"Agent 可驱动的跨网 E2E 投递 + 可信本地内容后端"这一定位无法完整成立：

1. **发现层断裂**：`get_info` 的 `with_instructions`（`src-tauri/src/mcp.rs:40-46`）只描述了发送三件套，未提 `search_inbox` / `get_inbox_file`——Agent 在握手阶段根本发现不了检索能力，整个"可检索后端"定位在第一跳就断了。
2. **生命周期只能发起、不能跟踪/干预**：Agent 能 `send_files` 发起传输，却无法列出/查询传输状态，也无法取消/暂停/恢复——一旦发起就失联。
3. **检索面不完整**：只能按关键词 `search_inbox`，不能列出最近收件箱；`search_inbox` 的 `include_archived` 在 MCP 层被硬编码为 `false`（`mcp/tools.rs`，而 core `inbox.rs:309` 与 Tauri 命令均已支持该参数）；缺少"按命中条目取完整详情"的下钻工具，search → 操作 的闭环不完整。

补齐这三处后，外部 Agent（Claude Desktop / Cursor / Claude Code 等支持本地 MCP 的客户端）即可端到端驱动 SwarmDrop 的传输生命周期与收件箱检索，且全部复用已存在的 core / Tauri 薄壳——零新增业务逻辑、零新增 Rust 依赖、不破坏"无云 / E2E / 仅 127.0.0.1"定位。

## What Changes

- **修复发现层缺陷**：`get_info` 的 server instructions 枚举完整工具集；按 MCP 规范为各工具补 `readOnlyHint` / `destructiveHint` 标注。
- **新增传输生命周期工具**（`mcp-transfer-tools` 新能力）：
  - `list_transfers` — 列出进行中 + 最近的传输会话（包裹 core `get_transfer_projections`）
  - `get_transfer_status` — 按 `session_id` 查单个会话的 phase / 进度 / 分文件状态
  - `cancel_transfer` / `pause_transfer` / `resume_transfer` — 包裹既有取消 / 暂停 / 恢复能力
  - `list_inbox` — 无关键词的最近收件箱列举（分页），与 `search_inbox` 互补
  - `get_inbox_item` — 按条目 id 取完整详情（包裹 core `get_inbox_item_detail`），补全 search → detail 闭环
- **扩展检索面**（`mcp-find-tools` 修改）：`search_inbox` 透传 `include_archived`。
- **更新 `swarmdrop://guide`**：补充新工具的用途、调用顺序与发现说明。

## Capabilities

### New Capabilities

- `mcp-transfer-tools`: Agent 驱动的传输生命周期工具集（列举 / 状态 / 取消 / 暂停 / 恢复）+ 收件箱列举与详情工具，以及完整的工具自描述 / 发现要求。

### Modified Capabilities

- `mcp-find-tools`: `search_inbox` 增加 `include_archived` 透传。

## Impact

- **依赖前置**：建在 `add-mcp-send-tools` 落地的 mcp-server 框架（rmcp `#[tool_router]` 注册、`McpHandler` 经 `AppHandle` 访问托管状态）与 `add-inbox-search-and-mcp-find` 的 `mcp-find-tools` 之上。本期复用其框架，不新增 MCP server 基础设施。
- **后端代码**：`src-tauri/src/mcp/tools.rs` 新增若干 `#[tool]` 方法；`src-tauri/src/mcp.rs` 的 `get_info().with_instructions` 更新。全部包裹既有 `src-tauri/src/commands/transfer.rs` / `commands/inbox.rs` 薄壳与 core `get_transfer_projections` / `get_inbox_item_detail`，无新增业务逻辑。
- **Rust 依赖**：无新增 crate。
- **共享 core**：本期不动 `crates/core`（MCP server 是桌面专属，`mobile-core` 无 axum / rmcp）。移动端的 Agent 驱动是另一条路线（App Intents / Shortcuts / 系统分享面板），不在本期。
- **文档**：更新 `src-tauri/docs/mcp-guide.md`（编译期 `include_str!` 嵌入为 `swarmdrop://guide` Resource）。
- **明确不在本期范围（Non-goals，见 AI 战略"不做清单"）**：声明式接收处置规则引擎；内容文本抽取 / `extracted_text` 回填；向量 / 语义检索；BLAKE3 精确去重；出站发送的二次确认 / 审计门控与 `forward_inbox_item`；跨设备检索；`.well-known` 发现 / 一键装机清单。这些是后续各自独立的 change，待 demand 验证后再立——不在本期堆叠，防范单人团队范围蔓延。
