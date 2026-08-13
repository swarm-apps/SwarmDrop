## ADDED Requirements

### Requirement: 每设备 MCP 代收门控 `allow_mcp_accept_from_device`

系统 SHALL 在每台已配对设备的接收策略（`receive_policy`）上提供布尔字段 `allow_mcp_accept_from_device`，语义为"允许 MCP/AI 代该来源设备处置入站 offer（接受或拒绝）"，与发送侧 `allow_mcp_send_to_device` 对称。该字段 SHALL 默认 `false`，SHALL 只能由用户在 SwarmDrop app 的设备信任策略中修改，SHALL NOT 经任何 MCP 工具被 agent 写改。`accept_transfer` 与 `reject_transfer` 两个工具 SHALL 均以该字段为前置门控。

#### Scenario: 默认不允许代收

- **WHEN** 一台设备完成配对但用户未开启 `allow_mcp_accept_from_device`
- **THEN** 该字段 SHALL 为 `false`，且对该来源的入站 offer，`accept_transfer` / `reject_transfer` SHALL 被拒绝执行

#### Scenario: 门控不可被 agent 提权

- **WHEN** agent 通过任何 MCP 工具尝试改变设备的接收策略
- **THEN** 系统 SHALL NOT 提供此能力（策略写操作保持 app-only），`allow_mcp_accept_from_device` 只能由 app 内用户操作变更

#### Scenario: 旧持久化数据向后兼容

- **WHEN** 反序列化本字段引入前已持久化的已配对设备
- **THEN** 缺失该字段的记录 SHALL 反序列化为 `allow_mcp_accept_from_device = false`，不破坏加载

### Requirement: accept_transfer MCP 工具

MCP Server SHALL 新增 `accept_transfer` Tool，按 `session_id` 接受一个处于 `RequireConfirmation` 挂起态（`phase=Offered / direction=receive`）的入站 offer。该 Tool SHALL 先由 `session_id` 定位挂起 offer 的来源设备并校验其 `allow_mcp_accept_from_device`，通过后复用 core 既有 `accept_and_start_receive`（通知对端、落盘、走状态机），SHALL NOT 在 MCP 层重写接收逻辑。该 Tool SHALL 接受可选 `save_path`（绝对路径）；未提供时 SHALL 落到**与手动接收一致的接收文件夹**（设置里配的接收位置，未配则 `<下载目录>/SwarmDrop`），SHALL NOT 使用独立的 agent 专用子目录。接受成功后 SHALL 把会话 `origin` 标记为 `Mcp{client}`，使收件箱条目以「AI 代理」来源区分。该 Tool 有落盘副作用，SHALL 标注 `read_only_hint=false`、`open_world_hint=true`。

#### Scenario: 授权后接受挂起 offer

- **WHEN** 来源设备已开启 `allow_mcp_accept_from_device`，agent 对一个 `phase=Offered` 的入站会话调用 `accept_transfer`
- **THEN** 系统 SHALL 接受该 offer、回复对端、按 `save_path`（或默认收件目录）落盘，并进入接收流程

#### Scenario: 未授权来源被门控拒绝

- **WHEN** 来源设备未开启 `allow_mcp_accept_from_device`，agent 调用 `accept_transfer`
- **THEN** Tool SHALL 返回 `isError: true`，提示需在 SwarmDrop 设备策略中开启"允许 MCP 代收"，且 SHALL NOT 接受该 offer

#### Scenario: 缺省保存位置

- **WHEN** agent 调用 `accept_transfer` 且未提供 `save_path`
- **THEN** 系统 SHALL 将文件落到与手动接收一致的接收文件夹（不存在则创建），并在返回中告知实际保存位置；且 SHALL 把会话 origin 标记为 Mcp，使该条目在收件箱显示为「AI 代理」来源

#### Scenario: 会话已过期或不存在

- **WHEN** 目标 offer 已被处理、已过挂起窗口被回收、或 `session_id` 无对应挂起 offer
- **THEN** Tool SHALL 返回 `isError: true` 说明会话已不可接受，SHALL NOT 使服务崩溃

#### Scenario: 节点或数据库未就绪

- **WHEN** 调用时 P2P 节点未启动或数据库连接尚未初始化
- **THEN** Tool SHALL 返回 `isError: true` 说明暂不可用

### Requirement: reject_transfer MCP 工具

MCP Server SHALL 新增 `reject_transfer` Tool，按 `session_id` 拒绝一个挂起的 `RequireConfirmation` 入站 offer。该 Tool SHALL 以来源设备的 `allow_mcp_accept_from_device` 为前置门控，通过后复用 core 既有 `reject_and_respond`（回复对端 `OfferResult{accepted:false}` 并走状态机）。该 Tool SHALL 标注 `destructiveHint`。

#### Scenario: 授权后拒绝挂起 offer

- **WHEN** 来源设备已开启 `allow_mcp_accept_from_device`，agent 对一个挂起 offer 调用 `reject_transfer`
- **THEN** 系统 SHALL 婉拒该 offer 并通知对端，会话置为拒绝态

#### Scenario: 未授权来源被门控拒绝

- **WHEN** 来源设备未开启 `allow_mcp_accept_from_device`
- **THEN** Tool SHALL 返回 `isError: true`，SHALL NOT 拒绝该 offer（决定权留给人）

#### Scenario: 会话已终态或不存在

- **WHEN** 目标 offer 已被处理或不存在
- **THEN** Tool SHALL 返回 `isError: true`，不重复拒绝、不使服务崩溃

### Requirement: 待审 offer 的发现与窗口约束自描述

系统 SHALL 使挂起的 `RequireConfirmation` 入站 offer 可被 agent 发现：此类会话 SHALL 以 `phase=Offered`、`direction=receive` 出现在既有 `list_transfers` 输出中，agent 据此轮询即可定位待审 offer。挂起 offer 的有效决策窗口受 libp2p 全局协议超时封顶（`req_resp_timeout` ≈ 180s，client 侧 `with_timeout` 无法加长），系统 SHALL 使接收端 pending 回收窗口 **小于**该协议窗口（SHALL NOT 晚于发送端放弃），避免"接收端已接受但发送端已放弃"竞态。`swarmdrop://guide` 与 `get_info` server instructions SHALL 说明"代收入站文件"的调用顺序（`list_transfers` 滤 `phase=Offered` → `accept_transfer` / `reject_transfer`），并 SHALL 显式声明该 ~180s（约 3 分钟）窗口、以及"要可靠代收需让 agent 处于活跃 watch 循环"这一模型边界。

#### Scenario: agent 发现待审 offer

- **WHEN** 存在一个挂起的 `RequireConfirmation` 入站 offer 且 agent 调用 `list_transfers`
- **THEN** 该 offer SHALL 以 `phase=Offered / direction=receive` 出现在结果中，供 agent 取其 `session_id` 做决定

#### Scenario: 指南披露窗口约束

- **WHEN** MCP 客户端读取 `swarmdrop://guide` 或 `get_info` instructions
- **THEN** 内容 SHALL 覆盖 accept/reject 工具的用途与顺序，并显式告知 ~180s（约 3 分钟）窗口与"需 watch 循环"的约束，而非让 agent 误以为可被动等待唤醒

#### Scenario: 工具携带能力提示

- **WHEN** 客户端调用 `tools/list`
- **THEN** `accept_transfer` SHALL 标注 `read_only_hint=false`、`reject_transfer` SHALL 标注 `destructiveHint`
