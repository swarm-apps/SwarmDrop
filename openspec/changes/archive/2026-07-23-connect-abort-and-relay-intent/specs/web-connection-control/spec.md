# web-connection-control

## ADDED Requirements

### Requirement: 任何 JS Promise 在有限时间内 settle

`WebNode` 暴露的所有异步方法 SHALL 保证 Promise 在有限时间内 settle（resolve 或 reject），不依赖调用方传入超时或信号。该不变量由内核兜底超时与状态机失败可观测共同保证；前端 SHALL 不再需要 `Promise.race` 型客户端兜底（删除 `with-timeout.ts`）。

#### Scenario: 无参调用也不会无限挂起

- **WHEN** 前端对不可达地址调用任一 `WebNode` 异步方法且不传任何超时/信号
- **THEN** Promise 在有限时间内（内核兜底上界）reject，附带可判别的错误信息

### Requirement: connect 支持标准 AbortSignal 取消

`WebNode.connect(addr, opts?)` SHALL 接受可选的标准 `AbortSignal`（`opts.signal`）；signal 触发时 Promise SHALL 立即以 abort 语义 reject。API SHALL 不自造 `timeoutMs` 类参数——超时组合由调用方经 `AbortSignal.timeout()` / `AbortSignal.any()` 表达。abort 不承诺撤回在途拨号（文档 SHALL 明示"abort ≠ 撤回拨号"），但 SHALL 保证无常驻意图残留。

#### Scenario: 调用方主动取消 connect

- **WHEN** `connect` 等待期间调用方 abort 传入的 signal
- **THEN** Promise 立即以 abort 语义 reject，后续该次调用不产生任何状态副作用

#### Scenario: 用平台原语表达超时

- **WHEN** 调用方以 `connect(addr, { signal: AbortSignal.timeout(5000) })` 调用且地址不可达
- **THEN** Promise 在约 5 秒时以 abort 语义 reject

### Requirement: relay 意图以声明式集合管理

`WebNode` SHALL 以命令/查询分离的形式管理 relay 可达意图，替代一次性 RPC 形态的 `reserve()`（删除该方法）：

- 命令：`relays_ensure(helper_addr)` 登记意图、`relays_drop(helper_id)` 撤销意图——二者 SHALL 同步返回、幂等，且 `relays_drop` SHALL 真正撤销内核常驻意图（联动 `remove_infrastructure_peer`），而非仅停止等待。
- 查询：`relays_state()` SHALL 返回全量 relay 状态快照（id、状态、circuit 地址、尝试次数等）；状态变化 SHALL 经 `relay-changed` 事件推送。

#### Scenario: ensure 立即返回，状态经订阅到达

- **WHEN** 前端调用 `relays_ensure(addr)`
- **THEN** 调用同步返回；reservation 建立后经 `relay-changed` 事件与 `relays_state()` 快照可观测到 `active` 状态及 circuit 地址

#### Scenario: drop 停止后台重试

- **WHEN** 某 helper 处于失败退避循环中，前端调用 `relays_drop(id)`
- **THEN** 内核不再对该 helper 重试，其条目从 `relays_state()` 快照中消失

#### Scenario: reservation 掉线可观测

- **WHEN** 已建立的 reservation 因 relay 断线失效
- **THEN** 前端经 `relay-changed` 事件观测到状态离开 `active`，无需重新发起调用

### Requirement: 提供可取消的"等待首次 Active"便捷方法

`WebNode` SHALL 提供 `relays_until_active(helper_id, opts?)`：等待指定 relay 首次进入 `active` 并 resolve 出 circuit 地址；接受可选 `AbortSignal`；观察到 `failed` 状态时 SHALL 立即 reject（携带失败原因），而非等待内核退避重试。该方法 SHALL 仅是状态订阅之上的便捷封装，不改变意图生命周期（reject/abort 不隐式撤销意图）。

#### Scenario: 等待成功

- **WHEN** `relays_ensure` 后调用 `relays_until_active(id)`，reservation 随后建立
- **THEN** Promise resolve 出该 relay 的 circuit 地址

#### Scenario: 失败快速反馈

- **WHEN** `relays_until_active(id)` 等待期间该 relay 进入 `failed`
- **THEN** Promise 立即 reject 并携带失败原因；意图仍保留，是否 `relays_drop` 由调用方决定
