# stale-receive-session-expiry Specification

## Purpose
TBD - created by archiving change harden-transfer-lifecycle. Update Purpose after archive.
## Requirements
### Requirement: 启动时回收过期未恢复的接收会话

系统 SHALL 在节点启动清理阶段,回收处于 `recoverable suspended` 状态、且自最后更新起已超过保留期(默认 7 天)仍未恢复的**接收方向**会话:将其转为 terminal 状态,并尽力清理其遗留的 `.part` 临时文件。该回收 MUST 不影响保留期内的会话(正常断点续传)、不影响发送方向会话、不影响非 recoverable 或已 terminal 的会话。

#### Scenario: 保留期内的挂起接收会话不被回收

- **WHEN** 一个 recoverable suspended 的接收会话最后更新时间在保留期内(如 3 天前)
- **THEN** 启动清理后该会话仍为 suspended/recoverable,可正常发起恢复,其 `.part` 文件保留

#### Scenario: 超过保留期的挂起接收会话被回收

- **WHEN** 一个 recoverable suspended 的接收会话最后更新时间超过保留期(如 8 天前)
- **THEN** 启动清理后该会话转为 terminal,不再出现在可恢复列表中,其遗留 `.part` 文件被删除(删除失败时仅记录告警,不阻断启动)

#### Scenario: 发送会话与终态会话不受回收影响

- **WHEN** 启动清理遇到发送方向的挂起会话,或已处于 terminal 的会话
- **THEN** 这些会话不被本回收逻辑改动

#### Scenario: 回收在两端启动路径一致执行

- **WHEN** 桌面端 `setup` 启动清理 或 移动端 `reconcile_stale_sessions` 运行
- **THEN** 两端都调用同一共享 core 清理原语,对过期接收会话应用相同的回收判定与终态转换

