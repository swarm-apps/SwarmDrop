## ADDED Requirements

### Requirement: 根据设备策略评估入站 offer
系统 SHALL 在接受或展示每个入站传输 offer 前，根据来源设备的信任层级和接收策略进行评估。

#### Scenario: 自有设备自动接收
- **WHEN** 入站 offer 来自策略允许自动接收的 `owned` 设备
- **THEN** 系统 SHALL 自动接受该 offer
- **AND** 接收到的内容 SHALL 进入收件箱

#### Scenario: 协作设备需要确认
- **WHEN** 入站 offer 来自默认策略下的 `collaborator` 设备
- **THEN** 系统 SHALL 展示常规接收确认界面
- **AND** 系统 SHALL NOT 在用户接受前开始接收

#### Scenario: 已屏蔽设备被拒绝
- **WHEN** 入站 offer 来自 `blocked` 设备
- **THEN** 系统 SHALL 拒绝该 offer
- **AND** 系统 SHALL 记录或展示策略拒绝原因

### Requirement: 执行传输限制
系统 SHALL 在自动接收入站 offer 前执行接收策略限制。

#### Scenario: Offer 超过最大大小
- **WHEN** 入站 offer 超过来源设备允许的最大传输大小
- **THEN** 系统 SHALL NOT 自动接受该 offer
- **AND** 系统 SHALL 根据策略要求确认或拒绝

#### Scenario: 目录传输被禁用
- **WHEN** 入站 offer 包含目录，且来源设备策略不允许目录
- **THEN** 系统 SHALL NOT 自动接受该 offer
- **AND** 系统 SHALL 根据策略要求确认或拒绝

#### Scenario: Relay 自动接收被禁用
- **WHEN** 入站 offer 将通过 Relay 接收，且来源设备策略不允许 Relay 自动接收
- **THEN** 系统 SHALL NOT 自动接受该 offer
- **AND** 系统 SHALL 改为要求用户确认

### Requirement: 自动接收经过收件箱路径
系统 SHALL 让自动接收的入站传输走收件箱接收路径。

#### Scenario: 自动接收传输完成
- **WHEN** 自动接收的入站传输成功完成
- **THEN** 系统 SHALL 为接收到的内容创建收件箱条目
- **AND** 活动与恢复记录 SHALL 标明本次传输使用了策略自动接收

#### Scenario: 自动接收传输失败
- **WHEN** 自动接收的入站传输失败或中断
- **THEN** 系统 SHALL 在活动与恢复中展示该传输
- **AND** 系统 SHALL NOT 创建收件箱条目，除非该传输稍后成功完成

### Requirement: 解释策略决策
系统 SHALL 向用户暴露策略决策结果。

#### Scenario: Offer 因策略自动接收
- **WHEN** 传输因为设备策略而自动开始
- **THEN** 系统 SHALL 显示它来自可信设备并已自动接收

#### Scenario: Offer 因策略被拒绝
- **WHEN** 传输因为设备策略被拒绝
- **THEN** 系统 SHALL 展示或记录策略原因

### Requirement: 未知策略使用安全回退
系统 SHALL 在无法解析策略时回退到需要确认。

#### Scenario: 策略不可用
- **WHEN** 入站 offer 到达，但系统无法加载来源 Peer 的策略
- **THEN** 系统 SHALL 要求用户确认
- **AND** 系统 SHALL NOT 自动接受该 offer
