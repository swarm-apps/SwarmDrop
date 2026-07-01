## ADDED Requirements

### Requirement: 终态写入单一入口与并发不可逆

系统 SHALL 把所有 session 级生命周期 phase 转换（包括 completed、failed、rejected、cancelled、suspended）统一经 `TransferCoordinator.dispatch` → `reduce` → `apply_transition` 落库，禁止任何路径直接写 session phase 绕过 reducer 的守卫。文件级副作用（落盘、checkpoint、inbox 索引）MAY 在 dispatch 之前完成，但 MUST NOT 自行决定 session 终态。

#### Scenario: 完成经状态机落库

- **WHEN** 发送方或接收方 actor 完成全部数据传输
- **THEN** actor MUST 先完成文件级副作用（finalize sink / 标记文件完成），再 `dispatch(Actor{epoch, Completed})` 由 reducer 写 phase=`terminal`/reason=`completed` 并发 projection
- **AND** MUST NOT 调用绕过 reducer 的直写函数（如 `mark_session_completed`）决定 session phase

#### Scenario: 取消后并发完成不覆盖终态

- **WHEN** 一个传输已被取消（reducer 已写 phase=`terminal`/reason=`cancelled`），随后一个并发的数据通道任务报告完成
- **THEN** 系统 MUST 因 terminal 不可逆守卫拒绝该完成转换（reduce 返回 None），session 终态 MUST 保持 `cancelled`
- **AND** 系统 MUST NOT 把已 `cancelled` 的 session 覆盖为 `completed`

#### Scenario: 旧 epoch 完成被忽略

- **WHEN** 一个旧 epoch 的 actor 在传输已恢复（epoch 已递增）之后才报告完成
- **THEN** 系统 MUST 因 epoch 守卫忽略该旧 epoch 的 actor 事件，不改变当前 epoch 的 session 状态

### Requirement: 前端传输状态以投影为唯一消费源

前端 SHALL 直接消费后端 `TransferProjection`（phase + reason + 实时 progress）渲染传输状态与文案，MUST NOT 维护把投影改名降维成旧扁平状态的有损适配层。

#### Scenario: 状态文案单一映射

- **WHEN** 前端需要把传输状态渲染成可读文案
- **THEN** 前端 MUST 由单一的 phase/reason→文案映射函数产出，MUST NOT 在多个组件内联各自的文案分支导致同一状态出现不一致表述
