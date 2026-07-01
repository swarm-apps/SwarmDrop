## ADDED Requirements

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
