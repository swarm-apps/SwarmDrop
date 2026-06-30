## ADDED Requirements

### Requirement: list_transfers MCP 工具

MCP Server SHALL 在已有的 mcp-server 框架（rmcp `#[tool_router]`、`McpHandler` 经 `AppHandle` 访问托管状态）上新增 `list_transfers` Tool，列出进行中与最近的传输会话。该 Tool SHALL 复用 core 的 `get_transfer_projections`（而非直连数据库），返回每个会话对 Agent 有意义的子集：session_id、direction（send / receive）、对端设备名、phase、可选 reason、总大小、已传字节 / 进度、文件数、起止时间；支持可选 `limit`。该 Tool SHALL NOT 暴露 `epoch`、内部 bitmap、`save_path` 等实现细节，并 SHALL 标注 `readOnlyHint`。

#### Scenario: Agent 查看传输列表

- **WHEN** MCP 客户端调用 `list_transfers`
- **THEN** 系统 SHALL 返回进行中与最近的会话列表（含 direction、对端、phase、进度）

#### Scenario: 无传输会话

- **WHEN** 当前没有任何传输会话
- **THEN** 系统 SHALL 返回空结果集且不报错

#### Scenario: 数据库未就绪

- **WHEN** 调用时数据库连接尚未初始化
- **THEN** Tool SHALL 返回 `isError: true` 说明暂不可用，而不是使服务崩溃

### Requirement: get_transfer_status MCP 工具

MCP Server SHALL 新增 `get_transfer_status` Tool，按 `session_id` 返回单个会话的详情：phase、可选 reason、整体进度，以及分文件状态（文件名、相对路径、大小、已传字节、状态）。复用 core 投影，SHALL NOT 直连数据库，SHALL 标注 `readOnlyHint`。

#### Scenario: 查询存在的会话

- **WHEN** Agent 用有效 `session_id` 调用 `get_transfer_status`
- **THEN** 系统 SHALL 返回该会话的 phase、进度与分文件状态

#### Scenario: 会话不存在

- **WHEN** 给定 `session_id` 在库中不存在
- **THEN** Tool SHALL 返回 `isError: true` 说明未找到对应会话

### Requirement: cancel_transfer MCP 工具

MCP Server SHALL 新增 `cancel_transfer` Tool，按 `session_id` 取消进行中的传输。该 Tool SHALL 复用既有取消能力（`cancel_send` / `cancel_receive`）——即通知对端并经 Coordinator 写入 `Cancelled`，使两端一致，SHALL NOT 仅做本地停止。该 Tool SHALL 标注 `destructiveHint`。

#### Scenario: 取消进行中的传输

- **WHEN** Agent 对一个进行中会话调用 `cancel_transfer`
- **THEN** 系统 SHALL 取消该会话、通知对端、并将两端状态置为 Cancelled

#### Scenario: 会话已终态或不存在

- **WHEN** 目标会话已是终态（completed / failed / cancelled）或不存在
- **THEN** Tool SHALL 返回当前状态或 `isError`，不重复取消、不使服务崩溃

### Requirement: pause_transfer / resume_transfer MCP 工具

MCP Server SHALL 新增 `pause_transfer` 与 `resume_transfer` Tool，按 `session_id` 暂停 / 恢复传输，复用既有暂停 / 恢复路径。恢复 SHALL 走既有的 `ResumeProbe → ResumeCommit → ResumeAck` 流程，MCP 工具仅作触发入口，SHALL NOT 引入新的状态转换或恢复分支。

#### Scenario: 暂停进行中的传输

- **WHEN** Agent 对进行中会话调用 `pause_transfer`
- **THEN** 系统 SHALL 将会话转为 suspended 并通知对端

#### Scenario: 恢复已暂停的传输

- **WHEN** Agent 对已暂停会话调用 `resume_transfer` 且对端在线
- **THEN** 系统 SHALL 经 Probe→Commit→Ack 恢复传输

#### Scenario: 恢复时对端不可用

- **WHEN** 恢复时对端不可达（`PeerUnavailable`）
- **THEN** Tool SHALL 如实回传"对端不可用、会话保留 suspended 供稍后重试"，SHALL NOT 报硬错或改坏本地状态

### Requirement: list_inbox MCP 工具

MCP Server SHALL 新增 `list_inbox` Tool，在不带关键词的情况下按接收时间倒序列出收件箱条目，支持 `limit` 与可选 `include_archived`（默认 false），与 `search_inbox` 互补。复用既有 inbox 列举能力，SHALL NOT 直连数据库，SHALL 标注 `readOnlyHint`。每个条目 SHALL 至少含 id、标题、来源设备名、接收时间、文件数。

#### Scenario: 列出最近收件箱

- **WHEN** Agent 调用 `list_inbox`
- **THEN** 系统 SHALL 按接收时间倒序返回收件箱条目列表

#### Scenario: 收件箱为空

- **WHEN** 收件箱无任何条目
- **THEN** 系统 SHALL 返回空结果集且不报错

### Requirement: get_inbox_item MCP 工具

MCP Server SHALL 新增 `get_inbox_item` Tool，按条目 id 返回完整详情，复用 core `get_inbox_item_detail`（与 `get_inbox_file` 同源）。返回 SHALL 含标题、来源设备名、接收时间，以及文件列表（每项含文件名、相对路径、大小、`missing` 标志、存在时的 `local_path`）。该 Tool 补全 `search_inbox` / `list_inbox` → 详情 → `get_inbox_file` 的检索闭环，SHALL 标注 `readOnlyHint`。

#### Scenario: 取条目完整详情

- **WHEN** Agent 用有效条目 id 调用 `get_inbox_item`
- **THEN** 系统 SHALL 返回该条目的标题、来源、接收时间与文件列表

#### Scenario: 条目不存在

- **WHEN** 给定条目 id 在库中不存在
- **THEN** Tool SHALL 返回 `isError: true` 说明未找到对应条目

#### Scenario: 条目含已缺失文件

- **WHEN** 条目下某文件 `missing` 为 true 或本地路径不可达
- **THEN** 该文件 SHALL 标记 `missing` 且不返回可被误用的 `local_path`

### Requirement: 工具自描述与发现完整性

MCP Server 的自描述（`get_info` 返回的 server instructions）与 `swarmdrop://guide` Resource SHALL 枚举完整工具集（发送、检索、传输生命周期、收件箱列举 / 详情），使 Agent 在握手与读取指南时即可发现全部能力。每个 Tool SHALL 按 MCP 规范声明能力提示：只读工具标 `readOnlyHint`；会取消传输的工具标 `destructiveHint`。

#### Scenario: 客户端握手发现全部工具

- **WHEN** MCP 客户端读取 `get_info` 的 instructions
- **THEN** instructions SHALL 覆盖全部已注册工具（含检索与传输生命周期工具），而非仅发送三件套

#### Scenario: 客户端读取指南

- **WHEN** MCP 客户端请求 `swarmdrop://guide`
- **THEN** 指南 SHALL 包含全部新工具的用途、参数与推荐调用顺序

#### Scenario: 工具携带能力提示

- **WHEN** 客户端调用 `tools/list`
- **THEN** 只读工具 SHALL 带 `readOnlyHint`、`cancel_transfer` SHALL 带 `destructiveHint`
