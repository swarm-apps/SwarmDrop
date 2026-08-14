## ADDED Requirements

### Requirement: ensure_node_running MCP 工具

MCP Server SHALL 新增 `ensure_node_running` Tool，使 agent 在无需人工前置的情况下让本机 P2P 节点上线。该 Tool SHALL 幂等：节点已在运行时直接返回当前网络状态、不重复启动；未运行时 SHALL 由后端自取已配对设备（keychain）作为启动参数（agent 不提供），复用既有 `start` 启动路径。该 Tool SHALL 以**设备身份已解锁**（keypair 已就绪于运行时状态）为门控；身份未就绪时 SHALL 返回 `isError` 并提示到 app 解锁，SHALL NOT 尝试触发生物识别/密码 UI。系统 SHALL NOT 提供经 MCP 停止节点的能力（`shutdown` 保持 app-only）。

#### Scenario: 节点未运行且身份已解锁时上线

- **WHEN** 身份已解锁、节点未运行，agent 调用 `ensure_node_running`
- **THEN** 系统 SHALL 用后端自取的已配对设备启动节点，并返回启动后的网络状态

#### Scenario: 节点已运行时幂等

- **WHEN** 节点已在运行，agent 调用 `ensure_node_running`
- **THEN** 系统 SHALL 直接返回当前网络状态（含 `alreadyRunning: true`），SHALL NOT 重复启动

#### Scenario: 身份未解锁

- **WHEN** 设备身份尚未初始化/解锁（keypair 未就绪于运行时状态）
- **THEN** Tool SHALL 返回 `isError: true` 提示到 app 解锁，SHALL NOT 尝试触发生物识别或密码 UI

#### Scenario: 不提供停止能力

- **WHEN** agent 尝试经 MCP 停止 / 下线本机节点
- **THEN** 系统 SHALL NOT 提供该能力（`shutdown` 仅限 app 内操作）

### Requirement: 接收暂停查询与开关 MCP 工具

MCP Server SHALL 新增 `get_receiving_paused` 与 `set_receiving_paused` Tool，包裹既有全局"暂停接收"能力（`is_receiving_paused` / `set_receiving_paused`）。`get_receiving_paused` SHALL 为只读并标注 `readOnlyHint`。`set_receiving_paused` SHALL 接受布尔 `paused`，暂停 SHALL 仅影响对新入站 offer 的自动婉拒，SHALL NOT 影响节点在线、配对或设备发现。

#### Scenario: 查询暂停状态消除盲区

- **WHEN** agent 调用 `get_receiving_paused`
- **THEN** 系统 SHALL 返回当前是否全局暂停接收，使 agent 能解释"为何发现不到入站 offer"

#### Scenario: 批处理前静音再恢复

- **WHEN** agent 调用 `set_receiving_paused(true)` 后再调用 `set_receiving_paused(false)`
- **THEN** 系统 SHALL 先对新入站 offer 自动婉拒、随后恢复按既有策略处理，全程不影响在线/配对/发现

#### Scenario: 节点未启动时的暂停查询

- **WHEN** P2P 节点未启动，agent 调用 `get_receiving_paused`
- **THEN** 系统 SHALL 返回"未暂停"（与既有命令行为一致），不报硬错
