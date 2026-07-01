# transfer-origin-provenance Specification

## Purpose
TBD - created by archiving change add-transfer-origin-provenance. Update Purpose after archive.
## Requirements
### Requirement: 传输来源模型（TransferOrigin）

系统 SHALL 定义一个 `TransferOrigin` 来源类型，区分两类传输发起方：人在应用内发起（`Human`）、AI 代理经 MCP 发起（`Mcp`，可携带 MCP 客户端名）。该来源 SHALL 作为**必填字段**承载于 wire 协议的传输 Offer 上、并贯穿到接收端的传输 Offer 事件，且 SHALL 持久化在传输会话记录的**独立字段**上（SHALL NOT 复用接收策略的 `policy_action` / `policy_reason`）。

#### Scenario: 人工发送携带 Human 来源

- **WHEN** 用户在应用内发起一次发送
- **THEN** 该传输的 Offer 与会话记录的来源 SHALL 为 `Human`

#### Scenario: MCP 发送携带 Mcp 来源

- **WHEN** AI 代理经 MCP `send_files` 发起一次发送
- **THEN** 该传输的 Offer 与会话记录的来源 SHALL 为 `Mcp`，并在可获取时带上 MCP 客户端名

### Requirement: 发送端标注来源与 MCP 客户端身份

发送路径 SHALL 在发起 Offer 时显式标注来源：应用内发送标注 `Human`，MCP `send_files` 标注 `Mcp`。对于 MCP 来源，系统 SHALL 尽力捕获连接的 MCP 客户端标识（来自 MCP `initialize` 握手的客户端名），并随来源一同记录；当无法获取客户端名时，SHALL 仍标注为 `Mcp`（客户端名为空），不阻塞发送。

#### Scenario: 捕获 MCP 客户端名

- **WHEN** 某 MCP 客户端（如 Claude Desktop）完成握手后调用 `send_files`
- **THEN** 该传输来源 SHALL 记录为 `Mcp` 且带该客户端名

#### Scenario: 客户端名不可得时降级

- **WHEN** 无法从握手获取客户端名
- **THEN** 来源 SHALL 仍为 `Mcp`（客户端名为空），发送照常进行

### Requirement: 发送端 MCP 门控

当一次发送的来源为 `Mcp` 时，系统 SHALL 在发送端校验目标设备的 `allow_mcp_send_to_device` 策略：仅当该策略为真时才允许发出 Offer。被拒时 SHALL 不发出 Offer，并向调用方（MCP 工具）返回清晰的错误，提示在设备策略中开启该项。`Human` 来源 SHALL NOT 受此门控影响。该门控 SHALL 是发送端控制，SHALL NOT 依赖接收端基于来源做二次拦截（来源为发送方自报、不可作为接收端的安全边界）。

#### Scenario: 目标设备允许 MCP 发送

- **WHEN** MCP 发起发送，且目标设备 `allow_mcp_send_to_device` 为真
- **THEN** 系统 SHALL 正常发出 Offer

#### Scenario: 目标设备不允许 MCP 发送

- **WHEN** MCP 发起发送，但目标设备 `allow_mcp_send_to_device` 为假
- **THEN** 系统 SHALL 拒绝发送、不发出 Offer，并返回说明"目标设备策略不允许 MCP 发送"的错误

#### Scenario: 人工发送不受门控

- **WHEN** 用户在应用内发起发送（来源 `Human`）
- **THEN** 系统 SHALL NOT 应用 `allow_mcp_send_to_device` 门控

### Requirement: 接收端展示来源

接收端 SHALL 把传输来源透传到接收 Offer 事件，使前端能在接受/拒绝对话框中标识发起方：`Mcp` 来源 SHALL 显示"由 AI 代理发起"（在可获取时附带客户端名），`Human` 来源不额外标注。该展示为信息性提示，接收决策仍由既有的接收策略与用户操作决定。

#### Scenario: 接收对话框标识 AI 代理发起

- **WHEN** 接收端收到来源为 `Mcp` 的 Offer
- **THEN** 接受/拒绝对话框 SHALL 标识"由 AI 代理发起"（有客户端名时一并显示）

#### Scenario: 人工发起不额外标注

- **WHEN** 接收端收到来源为 `Human` 的 Offer
- **THEN** 对话框 SHALL NOT 显示 AI 代理标识

### Requirement: 收件箱按来源派生 source_kind

接收完成写入收件箱时，系统 SHALL 依据接收会话的来源派生 `source_kind`：来源为 `Mcp` 的条目 SHALL 记为 `InboxSourceKind::Mcp`，否则记为 `PairedDevice`。收件箱 UI SHALL 对 `Mcp` 来源的条目显示来源标记，使用户事后仍能识别哪些文件是 AI 代理送达的。

#### Scenario: MCP 送达的文件在收件箱标记来源

- **WHEN** 一次来源为 `Mcp` 的接收完成并落入收件箱
- **THEN** 该收件箱条目的 `source_kind` SHALL 为 `Mcp`，且 UI SHALL 显示对应来源标记

#### Scenario: 人工送达的文件保持 PairedDevice

- **WHEN** 一次来源为 `Human` 的接收完成并落入收件箱
- **THEN** 该条目的 `source_kind` SHALL 为 `PairedDevice`

