# infra-peer-lifecycle Specification

## Purpose
TBD - created by archiving change connect-abort-and-relay-intent. Update Purpose after archive.
## Requirements
### Requirement: 基础设施节点登记具备对称注销面

`Endpoint` SHALL 提供 `remove_infrastructure_peer(node)`，与 `add_infrastructure_peer` 对称：从 relay 意图集合（`infra_relay_peers`）、地址簿、kad 路由表中摘除该节点，关闭其对应的 circuit listener，并从 `watch_relays` 状态表删除条目。注销后内核 SHALL 不再对该节点存在任何自动重连或 reservation 重建路径（identify 幂等重建、supervisor 收敛均不得复活已注销意图）。

#### Scenario: 注销后不再重试

- **WHEN** 某 relay helper 已经 `add_infrastructure_peer` 登记且拨号持续失败，调用方执行 `remove_infrastructure_peer`
- **THEN** 该节点从意图集合与地址簿移除，后续不再发生任何针对它的拨号或 reservation 请求，`watch_relays` 中其条目消失

#### Scenario: 注销活跃 reservation

- **WHEN** 某 relay 处于 `Active`（reservation 已建立）时被注销
- **THEN** 对应 circuit listener 被关闭，`watch_relays` 条目删除，且不触发 `Failed`/重建循环

### Requirement: 注销联动策略层清理

crates/core 的 `InfraSupervisor` 与 `BootstrapCandidateManager` SHALL 在基础设施节点注销时同步清除对应的收敛状态（`links` 条目）与候选表条目。注销后的"不复活"保证 SHALL 由收敛环闭合（见「收敛环双向收敛」），而非依赖调用方清理顺序或收敛任务内的候选表复查。学习型候选在该节点后续真实可达并经 identify 重新宣告时 MAY 重新纳管。

#### Scenario: supervisor 不复活已注销候选

- **WHEN** 一个处于退避重试中的 relay 候选被注销
- **THEN** 后续 supervisor tick 不再对其调用 `add_infrastructure_peer`，其 `links` 状态与候选条目均被清除

### Requirement: RelayState 状态机诚实反映失败

`RelayState` SHALL 至少区分三态：`Connecting`、`Active`（携带该 relay 的 circuit 可达地址）、`Failed`（携带末次错误）。拨号失败（该 peer 全部候选地址耗尽）与 reservation 失效 SHALL 翻转状态为 `Failed`；重新尝试时翻回 `Connecting`。观察者经 `watch_relays` SHALL 能区分"正在连接"与"连接失败退避中"。

`RelayState` SHALL NOT 携带重试轮数——轮数的语义由策略层退避规则定义，机制层无法自洽维护；重试记账 SHALL 由 `InfraSupervisor` 唯一持有（`links` 内账），诊断经 tracing 日志输出而非状态下发。

#### Scenario: 不可达 helper 进入 Failed

- **WHEN** 对一个不可达地址的 helper 登记 relay 意图，拨号失败（`OutgoingConnectionError`，全部候选地址耗尽）
- **THEN** `watch_relays` 中该节点状态变为 `Failed`，包含末次错误信息，而非停留在 `Connecting`

#### Scenario: circuit 地址由内核下发

- **WHEN** reservation 被 relay 接受，状态进入 `Active`
- **THEN** `Active` 状态携带完整 circuit 可达地址（`<relay>/p2p-circuit/p2p/<本机>`），调用方无需自行拼接

#### Scenario: 机制层不产生轮数

- **WHEN** 同一 relay 经 supervisor 退避多轮重试（多次 `add_infrastructure_peer` 重入、identify 幂等重建）
- **THEN** `RelayState` 各态均不含轮数字段，actor 侧不存在任何尝试计数器；supervisor 的 `links.attempts` 是重试轮数的唯一账本

### Requirement: connect 的等待具备内核兜底上界

`Endpoint::connect` SHALL 保持内建 `connect_timeout` 上界，且 Browser profile 下默认值 SHALL 不超过 15 秒——无论调用方是否传入取消信号，connect 的等待都在有限时间内结束。

#### Scenario: 浏览器 profile 默认超时

- **WHEN** 浏览器端点对不可达地址调用 `connect` 且调用方未设置任何超时
- **THEN** 调用在不超过 15 秒内以超时错误返回

### Requirement: 收敛环双向收敛

`InfraSupervisor` 的收敛 SHALL 是双向的：候选表存在应持有 reservation 的候选 → 收敛到内核已登记（正向，现状）；**内核 `watch_relays` 存在条目而候选表已无该 peer → 收敛到内核已注销**（反向）。反向规则 SHALL 以差集判据在每轮 tick 检查并幂等发出 `remove_infrastructure_peer`，直到内核条目消失——注销与在途注册任务的竞态由环的终态一致性闭合，SHALL NOT 依赖事件到达顺序或竞态窗口收窄。

该判据的前提 SHALL 成立并保持：候选表条目只经显式撤销移除（无自动过期清出），且所有生产路径的 relay 登记（bootstrap / mDNS LAN helper / 手动意图）均有候选表对应条目。引入候选自动清出机制前 MUST 重新评估本判据。

#### Scenario: 在途注册任务复活的登记被环清理

- **WHEN** 注销与一个在途的 `add_infrastructure_peer` 任务竞态，内核登记在注销后被短暂复活（`watch_relays` 重现该 peer 条目）
- **THEN** 后续 tick 经差集发现"内核有、候选无"，发出 `remove_infrastructure_peer`，有限轮内条目彻底消失

#### Scenario: 反向规则不误拆合法登记

- **WHEN** 候选表中存在某 relay 候选（无论 `public_reachability` 开关状态、无论其健康状态）
- **THEN** 反向规则不对该 peer 发出注销——差集只对"候选表完全没有该 peer"触发

#### Scenario: 用户撤销后快路径与环叠加

- **WHEN** 用户显式撤销 relay 意图（`remove_relay_intent`）
- **THEN** 直接注销调用立即生效（低延迟快路径），且即便该调用与在途注册竞态失败，反向环仍保证终态为已注销——二者幂等叠加

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

