# infra-peer-lifecycle

## MODIFIED Requirements

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

### Requirement: 注销联动策略层清理

crates/core 的 `InfraSupervisor` 与 `BootstrapCandidateManager` SHALL 在基础设施节点注销时同步清除对应的收敛状态（`links` 条目）与候选表条目。注销后的"不复活"保证 SHALL 由收敛环闭合（见「收敛环双向收敛」），而非依赖调用方清理顺序或收敛任务内的候选表复查。学习型候选在该节点后续真实可达并经 identify 重新宣告时 MAY 重新纳管。

#### Scenario: supervisor 不复活已注销候选

- **WHEN** 一个处于退避重试中的 relay 候选被注销
- **THEN** 后续 supervisor tick 不再对其调用 `add_infrastructure_peer`，其 `links` 状态与候选条目均被清除

## ADDED Requirements

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
