# web-connection-control

## MODIFIED Requirements

### Requirement: relay 意图以声明式集合管理

`WebNode` SHALL 以命令/查询分离的形式管理 relay 可达意图，替代一次性 RPC 形态的 `reserve()`（删除该方法）：

- 命令：`relays_ensure(helper_addr)` 登记意图、`relays_drop(helper_id)` 撤销意图——二者 SHALL 同步返回、幂等，且 `relays_drop` SHALL 真正撤销内核常驻意图（联动 `remove_infrastructure_peer`），而非仅停止等待。
- 查询：`relays_state()` SHALL 返回全量 relay 状态快照（id、状态、circuit 地址、失败原因）；状态变化 SHALL 经 `relay-changed` 事件推送。快照 SHALL NOT 包含重试轮数——轮数是策略层内账（supervisor 唯一持有），对 UI 无决策价值，诊断走日志。

#### Scenario: ensure 立即返回，状态经订阅到达

- **WHEN** 前端调用 `relays_ensure(addr)`
- **THEN** 调用同步返回；reservation 建立后经 `relay-changed` 事件与 `relays_state()` 快照可观测到 `active` 状态及 circuit 地址

#### Scenario: drop 停止后台重试

- **WHEN** 某 helper 处于失败退避循环中，前端调用 `relays_drop(id)`
- **THEN** 内核不再对该 helper 重试，其条目从 `relays_state()` 快照中消失

#### Scenario: reservation 掉线可观测

- **WHEN** 已建立的 reservation 因 relay 断线失效
- **THEN** 前端经 `relay-changed` 事件观测到状态离开 `active`，无需重新发起调用

#### Scenario: 失败快照含原因不含轮数

- **WHEN** 某 relay 意图处于失败退避中，前端读取 `relays_state()`
- **THEN** 该条目 state 为 `failed` 且携带末次错误描述（`lastError`），不存在 `attempts` 字段
