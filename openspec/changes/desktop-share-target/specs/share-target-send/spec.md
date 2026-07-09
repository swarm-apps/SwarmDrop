## ADDED Requirements

### Requirement: 从外部打开进入「文件已定」的反向发送屏

应用 SHALL 在收到 `external-file-open` 后，把路径扫描为待发文件并导航到独立整页路由 `/send/share-target`；但若设备尚未完成首启命名，则 MUST 丢弃本次意图并提示。

#### Scenario: 已命名 → 进入选设备屏

- **WHEN** 设备已设置名称，且收到一批被打开的文件路径
- **THEN** 应用对这些路径执行 `scan_sources`，把结果存入非持久的在途分享状态
- **AND** 导航到 `/send/share-target`，屏上已带好待发文件

#### Scenario: 未命名 → 丢弃并提示

- **WHEN** 设备尚未设置名称（仍在首启引导），收到被打开的文件
- **THEN** 应用提示「请先完成 SwarmDrop 设置」
- **AND** 丢弃本次意图，不导航、不缓冲留待稍后

### Requirement: 展示待发文件并支持逐项移除

share-target 屏 SHALL 展示待发文件的数量、总大小与结构（含文件夹层级），并允许发送前移除单项；移除文件夹级联移除其下文件。

#### Scenario: 展示数量与总大小

- **WHEN** 待发内容已就绪
- **THEN** 屏上显示项数与总大小（大小以等宽 tabular-nums 呈现），以及文件结构

#### Scenario: 移除文件夹级联移除子文件

- **WHEN** 用户移除一个文件夹项
- **THEN** 该文件夹下的所有文件一并从待发列表移除

#### Scenario: 全部移除后提示返回

- **WHEN** 用户移除了全部待发文件
- **THEN** 屏上提示已清空、需返回重新分享，且发送动作禁用

### Requirement: 选择一台在线已配对设备并发送

share-target 屏 SHALL 让用户从在线且可发送的已配对设备中单选一台并发出；节点未启动时自动启动；无可选设备时给出空状态。

#### Scenario: 节点未启动自动启动

- **WHEN** 进入 share-target 时 P2P 节点处于停止状态
- **THEN** 自动启动节点一次，设备区在启动完成前显示「正在启动节点…」占位

#### Scenario: 无在线可发送设备显示空状态

- **WHEN** 节点已运行但没有在线且可发送的已配对设备
- **THEN** 设备区显示空状态，提示让目标设备上线或先配对一台，本次分享可放弃

#### Scenario: 选设备并发送

- **WHEN** 用户选中一台在线设备并点击「发送给 X」
- **THEN** 依次执行 `prepare_send`（展示校验和准备进度）与 `start_send`
- **AND** 成功后导航到 `/transfer/$sessionId` 传输详情页

#### Scenario: 选中设备中途掉线

- **WHEN** 已选中的设备在发送前掉线或变为不可发送
- **THEN** 自动取消该选中，发送动作回落为「选择一个设备」

### Requirement: 复用既有发送链且不改动其行为

share-target SHALL 完全复用既有的 `scan_sources → prepare_send → start_send` 链，外部路径以 `FileSource::Path` 承载；不改动这些命令、传输核心或 `FileSource` 数据模型的行为。

#### Scenario: 外部路径经既有链发送

- **WHEN** share-target 用被打开的本地路径发起发送
- **THEN** 这些路径以 `FileSource::Path{path}` 传入现有 `scan_sources`，其余流程与交互式发送完全一致
