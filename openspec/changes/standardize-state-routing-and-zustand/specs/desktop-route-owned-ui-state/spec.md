## ADDED Requirements

### Requirement: 可导航 UI 状态必须由 route 拥有
桌面端所有可导航、可刷新恢复、可外部入口打开或应参与浏览历史的 UI 状态 MUST 由 TanStack Router path 或 search params 表达，而不是由 Zustand store 或不可恢复的组件局部状态表达。

#### Scenario: 传输详情可以刷新恢复
- **WHEN** 用户打开某个传输会话详情并刷新应用窗口
- **THEN** 应用 MUST 从当前 route 恢复到同一个传输详情，而不是退回列表或依赖内存 store 中的选中项

#### Scenario: 收件箱详情可以深链进入
- **WHEN** 外部入口或内部快捷入口导航到某个收件箱条目详情
- **THEN** 应用 MUST 使用 route path 或 search params 表达目标条目，并在页面加载时从数据源解析该条目

### Requirement: Store 不得拥有页面路由职责
桌面端 Zustand store MUST 只保存跨页面共享的领域状态、缓存、偏好或运行时状态；它 MUST NOT 保存当前子页面、当前详情页、当前选中路由实体等本应由路由表达的导航状态。

#### Scenario: 返回和前进由路由历史控制
- **WHEN** 用户在列表、详情、设置子页或收件箱子页之间导航后使用返回/前进
- **THEN** 应用 MUST 通过 router history 切换 UI，而不是通过 store 中的 selected/current 字段模拟页面跳转

#### Scenario: Store 只提供详情数据
- **WHEN** 某个详情 route 需要渲染传输、设备或收件箱实体
- **THEN** store MAY 提供实体缓存或加载 action，但当前详情目标 MUST 来自 route

### Requirement: 外部入口必须导航到标准 route
桌面端来自系统托盘、外部打开、传输完成快捷入口、通知或其他非页面来源的 UI 跳转 MUST 进入标准 route，而不是直接修改页面 store 来触发伪导航。

#### Scenario: 传输完成后进入收件箱
- **WHEN** 用户在传输详情中选择进入收件箱
- **THEN** 应用 MUST 导航到标准收件箱 route，并可通过 route 表达目标条目或筛选状态

#### Scenario: 托盘入口打开设置
- **WHEN** 用户通过托盘菜单打开设置
- **THEN** 应用 MUST 通过 router 导航到设置 route，而不是写入 store 字段让页面自行切换
