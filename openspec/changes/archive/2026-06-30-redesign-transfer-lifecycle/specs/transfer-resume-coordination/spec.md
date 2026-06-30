## ADDED Requirements

### Requirement: 所有恢复流程使用探测式协议
系统 SHALL 使用 `ResumeProbe`、`ResumeStateReport`、`ResumeCommit`、`ResumeAck` 完成恢复协商。

#### Scenario: 用户触发恢复
- **WHEN** 用户对 recoverable suspended session 点击恢复
- **THEN** 系统 MUST 向对端发送 `ResumeProbe`，并等待对端返回 `ResumeStateReport` 后再决定是否恢复

#### Scenario: 对端报告已取消
- **WHEN** `ResumeStateReport` 表示对端 session 已 terminal cancelled
- **THEN** 系统 MUST 拒绝恢复，并将本地 session 投影为 `terminal` 且 terminal reason 为 `cancelled`

### Requirement: 恢复提交生成新 epoch
系统 SHALL 在每次恢复提交时生成新的 epoch，并要求后续控制面和数据面消息携带该 epoch。

#### Scenario: 恢复被双方接受
- **WHEN** 双方状态报告兼容且 checkpoint 可以继续
- **THEN** 发起方 MUST 发送 `ResumeCommit`，其中包含 new_epoch、transfer key 信息和 fetch plan

#### Scenario: 收到恢复确认
- **WHEN** 对端接受 `ResumeCommit` 并返回 `ResumeAck`
- **THEN** 双方 MUST 将 session phase 更新为 `active`，并将 current epoch 更新为 new_epoch

### Requirement: 旧 epoch 消息不得更新状态
系统 SHALL 拒绝或忽略不匹配当前 epoch 的控制消息、actor 事件和数据通道握手。

#### Scenario: 旧 Complete 消息迟到
- **WHEN** 系统收到 epoch 小于 current epoch 的 `Complete` 消息
- **THEN** 系统 MUST 忽略该消息，并且 MUST NOT 将 session 标记为 completed

#### Scenario: 旧 actor 失败事件迟到
- **WHEN** 已被替换的旧 actor 报告失败事件
- **THEN** `TransferCoordinator` MUST 根据 actor epoch 忽略该事件

### Requirement: 恢复校验文件事实
系统 SHALL 在恢复协商中校验双方 manifest、checkpoint 和源文件事实。

#### Scenario: 源文件 fingerprint 不匹配
- **WHEN** 发送方报告的 source fingerprint 与会话 manifest 不兼容
- **THEN** 系统 MUST 拒绝恢复，并将 session 标记为不可恢复 fatal error

#### Scenario: Checkpoint 超出文件边界
- **WHEN** 接收方 checkpoint 指向不存在的文件、chunk 或 range
- **THEN** 系统 MUST 拒绝使用该 checkpoint，并要求重新开始或标记 fatal error
