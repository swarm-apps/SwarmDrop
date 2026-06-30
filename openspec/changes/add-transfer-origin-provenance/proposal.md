## Why

通过 MCP `send_files` 发起的传输，目前在接收端**与人手动发送完全无法区分**：

- wire 协议 `TransferRequest::Offer`（`crates/core/src/protocol.rs:111`）不带任何发起来源；
- `send_offer`（`crates/core/src/transfer/flow/send.rs:28`）对 UI 的 `start_send` 与 MCP 的 `send_files` 是同一函数、同样参数；
- 接收端拿到的 `TransferOfferEvent`（`incoming.rs:24`）和落库会话都没有来源信息；
- `receive_policy.allow_mcp_send_to_device`（`device.rs`）与 `InboxSourceKind::Mcp` 都是**声明了却从未使用的死字段**。

由此有两个真实缺口：（1）发送端**无法控制** agent 是否被允许往某台设备发文件（被注入/失控的 agent 静默外传风险）；（2）接收端的人/设备**无法知道**一次传输是 AI 代理发起的。用一个干净的"来源（origin）"抽象贯穿整条链路，一次补齐两者。

## What Changes

- 新增 `TransferOrigin` 枚举（`Human` / `Mcp { client }`），作为 wire `TransferRequest::Offer` 的**必填字段**（按决策**不做向后兼容**，两端同版本发布）、并贯穿到 `TransferOfferEvent`；在 `transfer_sessions` 用**专门的 `origin` 列**持久化（**不**复用 `policy_action`/`policy_reason`——那是接收策略语义）。
- 发送端标注来源：UI `start_send` → `Human`；MCP `send_files` → `Mcp { client }`，其中 `client` 是从 MCP `initialize` 握手捕获的连接客户端名（如 `claude-desktop`，拿不到则 `None`）。
- 发送端门控：当来源为 `Mcp` 时，`send_offer` 强制校验目标设备的 `allow_mcp_send_to_device`（接通现有死字段；默认值沿用 `device.rs`：Owned=true、其余=false）；被拒时 `send_files` 返回清晰错误。
- 接收端展示来源：offer 接受对话框标注"🤖 由 AI 代理发起（client）"；接收完成落 inbox 时按会话 origin 派生 `source_kind`（`Mcp` vs `PairedDevice`）；inbox UI 显示来源标记。
- 移动端（SwarmDrop-RN）消费同一份 core 改动：offer 对话框与 inbox 一并展示来源（RN 侧 UI 在该仓库完成）。

## Capabilities

### New Capabilities

- `transfer-origin-provenance`: 传输来源（人工 / MCP 代理）的端到端可追溯——`TransferOrigin` 模型、wire/事件/会话的贯穿、发送端 MCP 门控、接收端对话框与 inbox 的来源展示、MCP 客户端身份捕获。

## Impact

- **wire 协议（破坏性，按决策不兼容旧版）**：`TransferRequest::Offer` 新增必填 `origin: TransferOrigin`，与 `mobile-core` 共享同一份 core；新旧版本互联会反序列化失败——需两端同版本发布。
- **共享 core**：`protocol.rs`（enum + Offer）、`transfer/incoming.rs`（透传 origin → 事件 + 接收会话）、`transfer/flow/send.rs`（origin 参数 + 发送端门控）、`database/ops.rs`（`CreateSessionInput.origin` + 写列）、`database/inbox.rs`（`source_kind` 派生）、`device.rs`（接通既有 `allow_mcp_send_to_device`）。
- **数据库迁移**：`transfer_sessions` 新增 `origin` 列（`crates/migration` 加一条迁移并注册）。
- **桌面壳**：`src-tauri/src/mcp/tools.rs`（`send_files` 传 `Mcp { client }`）、`src-tauri/src/mcp.rs` + `mcp/server.rs`（`ServerHandler` 捕获 client `Implementation`）、`commands/transfer.rs`（`start_send` 传 `Human`）、`src/components/transfer/transfer-offer-dialog.tsx`、`src/routes/_app/inbox/index.lazy.tsx`。`src/lib/bindings.ts` 自动重生成。
- **移动端**：`mobile-core` wrapper 重建 + `src/components/transfer-offer-host.tsx` + inbox 展示（SwarmDrop-RN 仓库，core 改完"半免费"，UI 需手写）。
- **明确不做（Non-goals，守住简洁）**：
  - **接收端不基于 origin 二次拦截**——origin 是发送方自报、可伪造，接收端只**展示**不 gate（真正的控制在发送端门控）。
  - **不做每次发送的应用内二次确认**——per-device 的 `allow_mcp_send_to_device` 闸门已足够（对标 Web 权限模型：用户一次授权）。
  - **不捕获 agent 的模型/prompt**——工具层拿不到，最多到"MCP 客户端名"。
