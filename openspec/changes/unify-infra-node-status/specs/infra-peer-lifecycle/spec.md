## ADDED Requirements

### Requirement: 收敛按角色分档

`InfraSupervisor` 的收敛 SHALL 按角色分档，而非用单一判据决定整个候选是否入环：

- **DHT 种子角色**（`roles.kad_server`）SHALL 无条件收敛——它只需要把地址接进 kad 路由表，与「本机是否愿意被公网访问」正交。
- **中继角色**（`roles.relay_server`）SHALL 受 `public_reachability` 闸门约束（`Lan` scope 不受限，`Public` scope 需开关开启）。

启动路径的内核登记 SHALL 同样遵守该分档：以 `InfraRoles { kad_server: true, relay: false }` 登记，中继角色交由收敛环按闸门决策。SHALL NOT 在启动路径以 `relay: true` 无条件登记——那会绕过闸门。

SHALL NOT 把 `wants_reservation` 泛化为「是否入环」的总判据：那会使 `public_reachability=false` 时公网候选一次都不被登记，kad 路由表拿不到任何公网种子，DHT bootstrap 与在线记录发布全部失效。

#### Scenario: 关闭公网可达性不影响 DHT 种子

- **WHEN** `public_reachability` 为 false 且存在 `Public` scope 的 kad+relay 候选
- **THEN** 该候选仍被登记进 kad 路由表
- **AND** 不为其建立 relay reservation

#### Scenario: 启动路径不绕过闸门

- **WHEN** `public_reachability` 为 false 且节点启动并登记内置引导节点
- **THEN** 启动路径不为其建立 relay reservation

### Requirement: ever_active 是收敛状态的一部分

`InfraSupervisor` 的 `RelayLinkState` SHALL 携带 `ever_active: bool` 单调位：`RelayReservationAccepted` 时置位；候选被移除、节点停止、用户手动重启节点时清零。

该位 SHALL 经 `InfraLink` 下发。它与被禁止下发的 `attempts` / `next_attempt_at` 的区别是：它是**有明确清除条件的布尔事实**（「这条曾经成功过」），而非策略层的重试记账。

`attempts` 与 `next_attempt_at` SHALL 继续保持私有、不跨任何 IPC 边界。

#### Scenario: 首次成功后置位

- **WHEN** 某 relay 的 reservation 首次被接受
- **THEN** 其 `ever_active` 为 true

#### Scenario: 节点重启后清零

- **WHEN** 用户手动停止并重新启动节点
- **THEN** 全部 link 的 `ever_active` 为 false

#### Scenario: 快照仍不含轮数

- **WHEN** 读取任一 link 的对外快照
- **THEN** 其中不含 `attempts` 或 `next_attempt_at`

### Requirement: 基础设施意图登记面按角色泛化

crates/core SHALL 提供 `ensure_infra_intent(NodeAddr, CandidateRoles)` 与 `remove_infra_intent(NodeId)`，取代只能登记中继角色的 `ensure_relay_intent` / `remove_relay_intent`。

内核侧 SHALL NOT 需要新增泛化面——`Endpoint::add_infrastructure_peer(NodeAddr, InfraRoles)` 本就是泛化的，`ensure_relay_reservation` 是它的糖。

用户手动添加的引导节点 SHALL 以 `kad_and_relay` 角色登记。SHALL NOT 只登记中继角色——那会使该节点不进 kad 路由表。

#### Scenario: 手动添加的节点同时进路由表

- **WHEN** 用户添加一条引导节点
- **THEN** 该节点以 kad + relay 双角色登记
- **AND** 其地址进入 kad 路由表

### Requirement: 撤销入口受来源约束

`remove_infrastructure_peer` 的「立刻断开该节点全部连接」语义 SHALL 保持不变。

由于基础设施角色与设备身份可以重叠（局域网协助节点本身就是一台 SwarmDrop 设备，可能同时是已配对设备且正在传输文件），UI SHALL 仅对来源全部为 `HostConfigured` 的意图提供逐条撤销入口。自动来源（`MdnsLanHelper` / `Learned`）SHALL NOT 提供逐条撤销——撤销会断开在途传输，且该候选会在下一次 identify 时被重新登记。

#### Scenario: 自动候选不提供逐条撤销

- **WHEN** 某局域网协助节点由 mDNS 自动发现并登记
- **THEN** UI 不为其提供撤销入口

#### Scenario: 撤销仍立即断开

- **WHEN** 用户撤销一条 `HostConfigured` 的意图
- **THEN** 与该节点的全部连接立即断开，且后续不再重试
