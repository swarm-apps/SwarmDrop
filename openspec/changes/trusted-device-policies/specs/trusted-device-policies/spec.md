## ADDED Requirements

### Requirement: 分类可信设备
系统 SHALL 允许已知设备拥有持久化的信任层级。

#### Scenario: 旧配对设备获得默认信任
- **WHEN** 系统加载没有信任层级的已配对设备记录
- **THEN** 系统 SHALL 将其视为 `collaborator`
- **AND** 系统 SHALL 默认要求确认来自该设备的入站传输

#### Scenario: 用户标记自有设备
- **WHEN** 用户将某个已配对设备标记为自己的设备
- **THEN** 系统 SHALL 将设备信任层级持久化为 `owned`
- **AND** 系统 SHALL 在设备界面中展示该自有设备信任层级

#### Scenario: 用户屏蔽设备
- **WHEN** 用户将某个已知设备标记为已屏蔽
- **THEN** 系统 SHALL 将设备信任层级持久化为 `blocked`
- **AND** 系统 SHALL 阻止该设备自动向接收端发送内容

### Requirement: 按设备保存接收策略
系统 SHALL 为已知设备持久化接收策略设置。

#### Scenario: 用户编辑接收策略
- **WHEN** 用户修改某台设备的自动接收、最大大小、目录允许、Relay 自动接收或保存行为设置
- **THEN** 系统 SHALL 为该设备持久化更新后的策略
- **AND** 该设备未来的入站 offer SHALL 使用更新后的策略

#### Scenario: 策略缺失
- **WHEN** 设备没有显式策略
- **THEN** 系统 SHALL 使用该设备信任层级对应的默认策略模板

### Requirement: 提供安全默认模板
系统 SHALL 为每个信任层级提供安全的默认接收策略。

#### Scenario: 协作设备默认策略
- **WHEN** 设备被分类为 `collaborator`
- **THEN** 默认策略 SHALL 要求确认入站传输
- **AND** 默认策略 SHALL NOT 自动接收传输

#### Scenario: 自有设备默认策略
- **WHEN** 设备被分类为 `owned`
- **THEN** 默认策略 SHALL 支持将入站传输自动接收到收件箱
- **AND** 默认策略 SHALL NOT 自动接收经 Relay 路径到达的传输，除非用户显式启用

#### Scenario: 临时设备默认策略
- **WHEN** 设备被分类为 `temporary`
- **THEN** 默认策略 SHALL 要求过期时间或显式限制
- **AND** 默认策略 SHALL 要求确认，除非用户显式启用自动接收

### Requirement: 从设备界面编辑信任
系统 SHALL 在已配对设备界面暴露信任和策略控制。

#### Scenario: 配对后分类
- **WHEN** 新设备配对完成
- **THEN** 系统 SHALL 提供一种方式，让用户将设备分类为自有设备或协作设备

#### Scenario: 编辑已有设备策略
- **WHEN** 用户打开已配对设备的操作菜单或详情
- **THEN** 系统 SHALL 提供查看和编辑信任层级与接收策略的方式

### Requirement: 设备更新时保留策略
系统 SHALL 在运行时设备元数据变化时保留信任层级和接收策略。

#### Scenario: 设备名称变化
- **WHEN** 已配对设备的系统信息或显示名称变化
- **THEN** 系统 SHALL 更新设备元数据，并且不重置信任层级或接收策略
