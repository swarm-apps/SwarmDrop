# Design

## 背景与目标

本 change 是 SwarmDrop「AI 时代定位」战略的 NOW 阶段落地：用 AI **暴露**已有的 P2P / E2E / 无云护城河，而不是用 AI **创造**护城河。具体到工程，就是把已 ship 的 5 个 MCP 工具补成一个**完整可发现、可驱动全生命周期、可闭环检索**的 Agent 面，且不引入任何新业务逻辑或新依赖。

## 决策

### 决策 1：新增 `mcp-transfer-tools` 能力，而非塞进 `mcp-send-tools` / `mcp-find-tools`

传输**生命周期**（列举 / 状态 / 取消 / 暂停 / 恢复）与**收件箱列举 / 详情**是一组新的、内聚的 Agent 能力，与"发起发送"（send-tools）和"关键词检索"（find-tools）职责不同。单列一个能力符合项目的单一职责口味，也让后续归档进 `openspec/specs/` 时边界清晰。

### 决策 2：发现层修复纳入新能力的"自描述"需求，而不对 `mcp-server` 写 MODIFIED delta

`get_info` instructions 的漂移属于 `mcp-server` 能力，但 `mcp-server` / `mcp-send-tools` 仍在 `openspec/changes/add-mcp-send-tools/` **尚未归档进 `openspec/specs/`**（沿用 `add-inbox-search-and-mcp-find` 的先例：不对未归档 spec 写 MODIFIED delta）。因此把"自描述 / 发现完整性"作为**新工具可用性的内在要求**纳入 `mcp-transfer-tools`——新工具若不可被发现就等于不存在，这是本能力的合法需求，也精准对应战略洞察"发现层是整条定位的前提"。

### 决策 3：MCP 工具只做触发入口，业务逻辑全部复用既有薄壳

所有新工具都经 `McpHandler`（持 `AppHandle`）取托管状态后，调用**已存在**的实现，不在 MCP 层重写：

| MCP 工具 | 复用的既有实现 |
|---|---|
| `list_transfers` / `get_transfer_status` | core `get_transfer_projections`（`TransferProjection`） |
| `cancel_transfer` | 既有 `cancel_send` / `cancel_receive` 路径 |
| `pause_transfer` / `resume_transfer` | 既有暂停 / 恢复命令 |
| `list_inbox` | 既有 inbox 列举（`commands/inbox.rs`） |
| `get_inbox_item` | core `get_inbox_item_detail`（与 `get_inbox_file` 同源，无漂移） |
| `search_inbox`（改） | core `search_inbox(db, query, limit, include_archived)` |

这保证 MCP 面与前端 UI、Tauri 命令行为一致，零状态机分叉。

### 决策 4：取消 / 暂停 / 恢复严格走 Coordinator，MCP 不引入新状态转换

依据 `dev-notes/knowledge/rust-backend.md`：

- **取消不是本地停止**：必须通知对端 `TransferRequest::Cancel` + 经 Coordinator 写 `Cancelled`，两端一致。MCP `cancel_transfer` 直接复用 `cancel_send` / `cancel_receive`，不得只 `session.cancel()`。
- **恢复走 `ResumeProbe → ResumeCommit → ResumeAck`**，禁止新增 `ResumeRequest` / `ResumeOffer` 分支。MCP `resume_transfer` 仅触发既有恢复入口。
- 对端不可用（`PeerUnavailable`）时恢复不改本地状态、保留 suspended 供重试——MCP 工具如实回传该结果，不报硬错。

### 决策 5：`readOnlyHint` / `destructiveHint` 标注

- 只读：`list_transfers`、`get_transfer_status`、`list_inbox`、`get_inbox_item`、`search_inbox`、`get_inbox_file`、`get_network_status`、`list_available_devices` → `readOnlyHint`。
- 破坏性：`cancel_transfer`（中止传输）→ `destructiveHint`。`send_files`（对外投递副作用）维持现状，本期不改其确认语义（出站二次确认 / 审计是 NEXT 阶段独立 change）。
- `pause_transfer` / `resume_transfer` 既非只读也非破坏，按 MCP 默认（不标 readOnly、不标 destructive）。

### 决策 6：返回投影做裁剪，不泄露内部字段

`list_transfers` / `get_transfer_status` 返回 `TransferProjection` 的**对 Agent 有意义的子集**（session_id / direction / 对端名 / phase / reason / 进度 / 文件摘要），不暴露 `epoch`、内部 bitmap、`save_path` 等实现细节。输出走 camelCase JSON，与现有 5 个工具一致。

## Open Questions

- **分页**：`list_transfers` / `list_inbox` 起步沿用 `search_inbox` 的 limit-only 约定（无 offset）。若实际调用出现"翻页"需求再加 `offset`——不预先复杂化。
- **进行中 vs 历史的边界**：`list_transfers` 默认返回"进行中 + 最近 N 条"，N 取 limit；是否需要 `phase` 过滤参数（如只看 active）留待实现期按手感定，spec 不强制。
