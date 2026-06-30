## ADDED Requirements

### Requirement: NetworkStatus 新增引导节点连接状态
`NetworkStatus` 结构体 SHALL 新增 `bootstrapConnected: bool` 字段，表示是否至少有一个引导节点已连接。

#### Scenario: 引导节点已连接
- **WHEN** 事件循环检测到与引导节点 PeerId 的 ConnectionEstablished 事件
- **THEN** `bootstrapConnected` 设置为 `true`

#### Scenario: 引导节点断开
- **WHEN** 事件循环检测到所有引导节点的 ConnectionClosed 事件
- **THEN** `bootstrapConnected` 设置为 `false`

### Requirement: 桌面端设备页展示网络状态栏
桌面端设备页 SHALL 在 header 下方展示 `NetworkStatusBar`，与移动端共用同一组件。

#### Scenario: 桌面端在线时展示状态栏
- **WHEN** 节点处于 running 状态且在桌面端设备页
- **THEN** header 下方显示 NetworkStatusBar，展示网络状态信息

#### Scenario: 桌面端离线时展示空状态
- **WHEN** 节点未运行且在桌面端设备页
- **THEN** 显示 OfflineEmptyState（已有实现，保持不变）

### Requirement: NetworkStatusBar 展示详细网络状态
`NetworkStatusBar` SHALL 在节点运行时展示以下状态指示器：引导节点连接状态、Relay 中继就绪状态、NAT 穿透状态。

#### Scenario: 所有状态正常
- **WHEN** bootstrapConnected=true, relayReady=true, natStatus="public"
- **THEN** 三项状态指示器均显示为正常（绿色）

#### Scenario: 部分状态异常
- **WHEN** bootstrapConnected=true, relayReady=false, natStatus="unknown"
- **THEN** 引导节点显示正常，中继和 NAT 显示为警告状态

#### Scenario: 引导节点未连接
- **WHEN** bootstrapConnected=false
- **THEN** 引导节点状态显示为异常（红色/警告）
