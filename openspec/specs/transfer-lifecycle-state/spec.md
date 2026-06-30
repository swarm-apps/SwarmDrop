# transfer-lifecycle-state Specification

## Purpose
TBD - created by archiving change cleanup-transfer-tech-debt. Update Purpose after archive.
## Requirements
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

### Requirement: 传输状态由 Coordinator 统一管理
系统 SHALL 通过后端 `TransferCoordinator` 统一管理传输生命周期状态、运行时 actor 注册、DB 投影和前端事件。

#### Scenario: 用户暂停传输
- **WHEN** 用户请求暂停 active 传输
- **THEN** 系统 MUST 由 `TransferCoordinator` 驱动 actor flush checkpoint，并将 session 投影为 `suspended` 且 reason 为 `local_paused`

#### Scenario: Actor 不直接决定终态
- **WHEN** SenderActor 或 ReceiverActor 发生错误、完成或取消
- **THEN** actor MUST 将事件报告给 `TransferCoordinator`，并且 MUST NOT 绕过 Coordinator 直接决定 session phase

### Requirement: 暂停、中断、失败、取消语义分离
系统 SHALL 明确区分用户暂停、异常中断、不可恢复失败和用户取消。

#### Scenario: 网络断开导致中断
- **WHEN** active 传输的 peer 连接断开或数据请求因连接丢失失败
- **THEN** 系统 MUST 将 session 投影为 `suspended` 且 reason 为 `interrupted` 或 `peer_offline`，并保持 recoverable=true

#### Scenario: 用户取消传输
- **WHEN** 用户取消传输
- **THEN** 系统 MUST 将 session 投影为 `terminal` 且 terminal reason 为 `cancelled`，并且后续恢复请求 MUST 被拒绝

#### Scenario: 不可恢复错误
- **WHEN** 源文件变更、文件校验失败或协议不兼容导致无法继续
- **THEN** 系统 MUST 将 session 投影为 `terminal` 且 terminal reason 为 `fatal_error`

### Requirement: 启动清理保持可恢复事实
系统 SHALL 在应用启动时清理旧运行时状态，但不得把可恢复中断错误归类为用户暂停或不可恢复失败。

#### Scenario: 启动发现发送方 active 会话
- **WHEN** 应用启动时发现本地 DB 中存在 sender 方向的 active session
- **THEN** 系统 MUST 将其投影为 `suspended` 且 reason 为 `app_restarted`，除非该 session 已有 terminal marker

#### Scenario: 启动发现接收方有 checkpoint
- **WHEN** 应用启动时发现 receiver 方向的 active session 且存在 checkpoint 进度
- **THEN** 系统 MUST 将其投影为 `suspended` 且 reason 为 `app_restarted`，并保留 checkpoint

### Requirement: 前端只渲染后端投影
前端 SHALL 以后端 transfer projection 作为活跃传输和历史记录的唯一状态来源。

#### Scenario: 前端加载传输列表
- **WHEN** 前端打开传输列表
- **THEN** 前端 MUST 从后端 projection API 获取传输项，并按 projection 中的 phase/reason 渲染文案和按钮

#### Scenario: 前端收到进度事件
- **WHEN** 前端收到 transfer progress 事件
- **THEN** 前端 MUST 只更新对应 projection 的进度字段，并且 MUST NOT 自行推断 terminal 或 suspended 状态

