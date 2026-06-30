# home-device-hub Specification

## Purpose
TBD - created by archiving change redesign-home-device-hub. Update Purpose after archive.
## Requirements
### Requirement: 首页展示附近设备
桌面端首页 SHALL 展示附近未配对且在线的设备列表，使用户无需打开二级菜单即可发现可配对设备。

#### Scenario: 展示附近未配对在线设备
- **WHEN** 网络节点处于运行中，且 `network-store.devices` 中存在 `isPaired=false` 且 `status="online"` 的设备
- **THEN** 首页 SHALL 在“附近设备”区域展示这些设备

#### Scenario: 附近设备为空
- **WHEN** 网络节点处于运行中，且没有附近未配对在线设备
- **THEN** 首页 SHALL 展示“暂无附近设备”类空状态，并提示确认对端 SwarmDrop 已启动

#### Scenario: 从附近设备发起配对
- **WHEN** 用户在“附近设备”区域点击某个未配对设备的配对操作
- **THEN** 系统 MUST 调用现有直接配对流程并传入该设备的 `peerId`

### Requirement: 首页展示已配对设备
桌面端首页 SHALL 展示已配对设备，并将在线设备作为主要发送目标。

#### Scenario: 合并在线与离线已配对设备
- **WHEN** 用户打开首页
- **THEN** 首页 SHALL 使用运行时设备数据覆盖同 peer 的持久化设备数据，并为未在线发现的已配对设备展示离线状态

#### Scenario: 在线设备优先
- **WHEN** 首页同时存在在线和离线已配对设备
- **THEN** 首页 SHALL 将在线已配对设备排列在离线已配对设备之前

#### Scenario: 向在线已配对设备发送文件
- **WHEN** 用户点击在线已配对设备的发送操作
- **THEN** 系统 MUST 导航到 `/send` 并携带该设备的 `peerId`

#### Scenario: 离线已配对设备不可发送
- **WHEN** 已配对设备处于离线状态
- **THEN** 首页 SHALL 禁用或隐藏该设备的发送操作，并保留设备识别信息

### Requirement: 首页提供快速配对入口
桌面端首页 SHALL 提供生成配对码和输入配对码两个明确入口。

#### Scenario: 生成配对码入口
- **WHEN** 用户点击“生成配对码”
- **THEN** 系统 MUST 导航到 `/pairing/generate`

#### Scenario: 输入配对码入口
- **WHEN** 用户点击“输入配对码”
- **THEN** 系统 MUST 导航到 `/pairing/input`

### Requirement: 首页仅展示活跃传输
桌面端首页 SHALL 只展示正在等待确认、准备中或传输中的活跃传输会话，不展示历史传输记录。

#### Scenario: 展示活跃传输
- **WHEN** `transfer-store.sessions` 中存在活跃会话
- **THEN** 首页 SHALL 在“正在传输”区域按开始时间倒序展示这些会话

#### Scenario: 活跃传输为空
- **WHEN** `transfer-store.sessions` 为空
- **THEN** 首页 SHALL 展示轻量空状态或保持该区域为低占用状态

#### Scenario: 不展示历史传输
- **WHEN** `transfer-store.dbHistory` 中存在历史记录
- **THEN** 首页 MUST NOT 渲染最近传输历史列表

### Requirement: 顶栏提供传输历史入口
桌面端顶栏 SHALL 提供稳定的传输历史入口，并放置在设置入口左侧。

#### Scenario: 点击历史入口
- **WHEN** 用户点击顶栏历史图标
- **THEN** 系统 MUST 导航到 `/transfer`

#### Scenario: 历史入口位置
- **WHEN** 顶栏渲染在非全屏应用路由中
- **THEN** 历史入口 SHALL 显示在设置入口左侧，并位于窗口控制按钮之前

### Requirement: 保留离线节点流程
桌面端首页 SHALL 在网络节点未运行时保留离线空状态和启动节点入口。

#### Scenario: 节点未运行
- **WHEN** 网络节点状态不是 `running` 或 `starting`
- **THEN** 首页 SHALL 展示离线空状态，并提供启动节点入口

#### Scenario: 节点启动后进入设备中心
- **WHEN** 用户从离线空状态启动节点且节点进入运行中
- **THEN** 首页 SHALL 展示设备中心内容，包括附近设备、已配对设备、快速配对和正在传输区域

