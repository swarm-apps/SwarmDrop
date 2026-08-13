## ADDED Requirements

### Requirement: 将传输过程与收件箱内容分开展示
系统 SHALL 在独立于 Drop Inbox 的活动与恢复界面中展示传输过程状态。

#### Scenario: 活跃传输显示在活动中
- **WHEN** 一个传输当前处于活跃状态
- **THEN** 系统 SHALL 在活动与恢复中展示该传输的进度、对端、方向和当前文件上下文

#### Scenario: 已完成的入站内容主要显示在收件箱
- **WHEN** 一个入站传输完成并创建了收件箱条目
- **THEN** 已接收内容 SHALL 作为主要用户可见记录显示在收件箱中
- **AND** 活动与恢复 SHALL 将任何关联的已完成过程记录视为次要诊断上下文

### Requirement: 暴露可恢复传输
系统 SHALL 识别可以恢复的暂停或中断传输。

#### Scenario: 可恢复传输显示在恢复区
- **WHEN** 一个传输会话处于 suspended 且 recoverable
- **THEN** 系统 SHALL 在活动与恢复中将其显示为可恢复
- **AND** 当所需对端和本地状态可用时，系统 SHALL 提供恢复操作

#### Scenario: 不可恢复失败仅作为诊断记录
- **WHEN** 一个传输进入不可恢复的 terminal failed 状态
- **THEN** 系统 SHALL 在活动与恢复中展示失败原因
- **AND** 系统 SHALL NOT 展示恢复操作

### Requirement: 保留用于诊断和恢复的过程账本
系统 SHALL 保留传输会话和文件记录，作为进度、恢复和诊断的过程账本。

#### Scenario: 收件箱条目引用传输会话
- **WHEN** 从已完成接收会话创建收件箱条目
- **THEN** 系统 SHALL 保留传输会话引用，用于来源和诊断详情

#### Scenario: 清空活动不删除收件箱
- **WHEN** 用户清空活动与恢复记录
- **THEN** 系统 SHALL NOT 删除收件箱条目或本地已接收文件

### Requirement: 解释传输结果
系统 SHALL 为传输记录暴露清晰的结果标签和原因。

#### Scenario: Suspended 传输有原因
- **WHEN** 一个传输处于 suspended
- **THEN** 系统 SHALL 在已知时展示原因是本地暂停、远端暂停、中断、对端离线还是应用重启

#### Scenario: Terminal 传输有原因
- **WHEN** 一个传输已完成、取消、拒绝或失败
- **THEN** 系统 SHALL 展示 terminal 结果以及可用的错误信息或取消原因

### Requirement: 避免重复主列表记录
系统 SHALL 避免将同一个已完成入站内容作为两个同级主记录展示。

#### Scenario: 已完成入站传输同时存在账本和收件箱记录
- **WHEN** 一个已完成入站传输已经有关联的收件箱条目
- **THEN** 系统 SHALL 使用收件箱作为主要内容记录
- **AND** 系统 SHALL 将活动条目视为次要过程详情
