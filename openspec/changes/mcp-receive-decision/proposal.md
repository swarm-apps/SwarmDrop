## Why

SwarmDrop 的本地 MCP Server 现有 12 个工具，覆盖的是 app 的一个**单向偏发起端切片**：对外发送（`send_files`，受 `allow_mcp_send_to_device` 门控）+ 传输可观测/控制 + 收件箱只读。整条**接收侧决策链是零覆盖**——入站文件到达时，策略闸 `evaluate_receive_policy` 若判为 `RequireConfirmation`，offer 会挂起等一个"接受/拒绝 + 存哪"的决定，而这个决定**只有人能在 SwarmDrop app 的弹窗里做**。

后果：agent 只能在文件**已经落盘之后**只读 inbox，无法影响"是否落盘"。在 SwarmDrop「设备间数据通道（人 + Agent）」的定位下，agent 是一个**不对称参与者**——能发、能读，唯独不能"收"。要让 agent 成为对称 peer（"帮我盯着，来文件就按规则收下 + 处理"），缺的正是这块**接收侧决策权**。

前置 change `expand-mcp-agent-tools` 当年**故意**把接收处置划进 Non-goals（"声明式接收处置规则引擎""出站二次确认/审计门控"都排除了）。所以这不是补它的尾巴，而是它留白的一块新地——单独立案。

## What Changes

- **新增每设备接收门控字段** `allow_mcp_accept_from_device`（住在 `receive_policy`，与发送侧 `allow_mcp_send_to_device` 对称）：默认 false，**只能人在 app 的设备策略里开**，agent 不能自己翻。它同时门控 `accept_transfer` 与 `reject_transfer`——即"是否允许 MCP 代该设备处置入站 offer"。
- **新增 `accept_transfer` MCP 工具**：按 `session_id` 接受一个挂起的 `RequireConfirmation` 入站 offer，复用 core 既有 `accept_and_start_receive`。`save_path` 可选，缺省落到**与手动接收一致的接收文件夹**（设置里的 `preferences.transfer.savePath`，未配则 `<下载目录>/SwarmDrop`）。代收的文件在收件箱标记为「AI 代理」来源区分，不搞独立目录。
- **新增 `reject_transfer` MCP 工具**：按 `session_id` 拒绝挂起的入站 offer，复用 core 既有 `reject_and_respond`。
- **待审 offer 的发现**：明确 pending 的 `RequireConfirmation` offer 以 `phase=Offered / direction=receive` 出现在既有 `list_transfers` 中，agent 据此轮询发现；`swarmdrop://guide` 补充"代收要进 watch 循环"的用法说明。
- **对齐决策窗口到协议上限 ~180s**：Offer 请求受 libp2p 全局 `req_resp_timeout`（实为 180s）封顶，client 侧 `with_timeout` 加长不了；把接收端 `PENDING_OFFER_TIMEOUT_SECS` 设为略小于它（170s），消除"接收端已接受但发送端已放弃"的竞态，给 agent/人 ~180s（约 3 分钟）决策窗口。
- **更新 `swarmdrop://guide`**：新增"代收入站文件"小节 + 工具表扩到 20 个（与并行的 mcp-agent-autonomy-controls 合并落地），并**显式写出 ~180s（约 3 分钟）offer 窗口约束**。

## Capabilities

### New Capabilities

- `mcp-receive-tools`: Agent 驱动的入站接收决策工具集（`accept_transfer` / `reject_transfer`）、每设备 MCP 代收门控 `allow_mcp_accept_from_device`、与手动一致的默认收件文件夹 + 收件箱「AI 代理」来源标记、待审 offer 的发现契约，以及"代收需 watch 循环 / ~180s 窗口"的自描述要求。

## Impact

- **门控模型**：`accept_transfer` / `reject_transfer` 在**工具层**检查 `allow_mcp_accept_from_device`（镜像 `send_files` 在 `tools.rs` 检查 `allow_mcp_send_to_device` 的做法），**不改** `evaluate_receive_policy`。能走到 pending `RequireConfirmation` 的 offer 已过 `max_transfer_bytes` / `allow_directories` / `Blocked` / 过期 各闸，故 agent-accept **自动继承**这些上限，无需重复校验。
- **后端代码**：`src-tauri/src/mcp/tools.rs` 新增 `accept_transfer` / `reject_transfer` 两个 `#[tool]`；复用 core `crates/core/src/transfer/flow/receive.rs` 的 `accept_and_start_receive` / `reject_and_respond`（**不改** core 传输逻辑）。`receive_policy` 结构新增布尔字段 `allow_mcp_accept_from_device`（core `crates/core/src/device.rs` 或策略结构所在处）+ app 端设备策略 UI 加一个开关。
- **保存位置**：新增一个 MCP 默认收件目录解析（preference 或常量），`accept_transfer` 缺省用它，走现成 `CoreSaveLocation::Path`。
- **传输/协议**：**零协议改动**、**不需要开 rmcp `elicitation` feature**（本期是 agent 自主 pull 模型，非 server 主动推审批）。
- **共享 core**：`receive_policy` 字段在 `crates/core`，两端（桌面 + SwarmDrop-RN）共用；MCP 工具是桌面专属（`mobile-core` 无 rmcp）。
- **已知约束（写进 spec 与 guide，不隐藏）**：有效决策窗口受 libp2p 全局协议超时封顶 **≈ 180s（约 3 分钟）**——`with_timeout(300s)` 是 no-op、加长不了（详见 design 决策 5 的实现修正）；接收端 pending 回收窗口取 170s < 180s 以避免竞态。即便有 ~3 分钟，"agent 自主代收"仍是**常驻/循环 agent** 能力——一次性 agent 若不在轮询循环里仍会错过窗口，这是模型边界，非缺陷。要真正更长的窗口需协议层改造（Offer 先 ack + 独立决策通道），另立 change。
- **明确不在本期范围（Non-goals）**：server 主动 elicitation 推审批给人/client（Claude Desktop 不支持、需捕获会话 peer 句柄，另议）；server→client notification 主动"戳"agent（多数 client 收到通知不会自动唤起模型，留 Open Question）；声明式接收规则引擎 / 自动接收窗口；出站 `forward_inbox_item`。
