# infra-peer-lifecycle

## ADDED Requirements

### Requirement: 基础设施节点登记具备对称注销面

`Endpoint` SHALL 提供 `remove_infrastructure_peer(node)`，与 `add_infrastructure_peer` 对称：从 relay 意图集合（`infra_relay_peers`）、地址簿、kad 路由表中摘除该节点，关闭其对应的 circuit listener，并从 `watch_relays` 状态表删除条目。注销后内核 SHALL 不再对该节点存在任何自动重连或 reservation 重建路径（identify 幂等重建、supervisor 收敛均不得复活已注销意图）。

#### Scenario: 注销后不再重试

- **WHEN** 某 relay helper 已经 `add_infrastructure_peer` 登记且拨号持续失败，调用方执行 `remove_infrastructure_peer`
- **THEN** 该节点从意图集合与地址簿移除，后续不再发生任何针对它的拨号或 reservation 请求，`watch_relays` 中其条目消失

#### Scenario: 注销活跃 reservation

- **WHEN** 某 relay 处于 `Active`（reservation 已建立）时被注销
- **THEN** 对应 circuit listener 被关闭，`watch_relays` 条目删除，且不触发 `Failed`/重建循环

### Requirement: 注销联动策略层清理

crates/core 的 `InfraSupervisor` 与 `BootstrapCandidateManager` SHALL 在基础设施节点注销时同步清除对应的收敛状态（`links` 条目）与候选表条目，使 supervisor tick 不再对已注销节点发起收敛。学习型候选在该节点后续真实可达并经 identify 重新宣告时 MAY 重新纳管。

#### Scenario: supervisor 不复活已注销候选

- **WHEN** 一个处于退避重试中的 relay 候选被注销
- **THEN** 后续 supervisor tick 不再对其调用 `add_infrastructure_peer`，其 `links` 状态与候选条目均被清除

### Requirement: RelayState 状态机诚实反映失败

`RelayState` SHALL 至少区分三态：`Connecting`（含尝试次数）、`Active`（携带该 relay 的 circuit 可达地址）、`Failed`（携带累计尝试次数、末次错误、下次重试时间）。拨号失败（该 peer 全部候选地址耗尽）与 reservation 失效 SHALL 翻转状态为 `Failed`；重新尝试时翻回 `Connecting`。观察者经 `watch_relays` SHALL 能区分"正在连接"与"连接失败退避中"。

#### Scenario: 不可达 helper 进入 Failed

- **WHEN** 对一个不可达地址的 helper 登记 relay 意图，拨号失败（`OutgoingConnectionError`，全部候选地址耗尽）
- **THEN** `watch_relays` 中该节点状态变为 `Failed`，包含末次错误信息，而非停留在 `Connecting`

#### Scenario: circuit 地址由内核下发

- **WHEN** reservation 被 relay 接受，状态进入 `Active`
- **THEN** `Active` 状态携带完整 circuit 可达地址（`<relay>/p2p-circuit/p2p/<本机>`），调用方无需自行拼接

### Requirement: connect 的等待具备内核兜底上界

`Endpoint::connect` SHALL 保持内建 `connect_timeout` 上界，且 Browser profile 下默认值 SHALL 不超过 15 秒——无论调用方是否传入取消信号，connect 的等待都在有限时间内结束。

#### Scenario: 浏览器 profile 默认超时

- **WHEN** 浏览器端点对不可达地址调用 `connect` 且调用方未设置任何超时
- **THEN** 调用在不超过 15 秒内以超时错误返回
