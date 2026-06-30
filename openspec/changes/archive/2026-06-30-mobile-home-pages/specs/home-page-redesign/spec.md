## ADDED Requirements

### Requirement: Mobile devices page layout
设备首页在移动端断点（<768px）时 SHALL 渲染移动端专属布局，包含以下区域（从上到下）：内联网络状态条、内容区（已配对设备 + 附近设备，或离线空状态）。页面头部（"SwarmDrop" 标题 + ➕ 按钮）由 `_app.tsx` 的移动端布局提供，不在设备页面内部。

#### Scenario: Mobile online state renders device lists
- **WHEN** 用户在移动端访问设备页面且节点状态为 "running"
- **THEN** 页面顶部显示绿色网络状态条，下方依次显示"已配对设备"区块和"附近设备"区块，设备以列表样式展示

#### Scenario: Mobile offline state renders empty state
- **WHEN** 用户在移动端访问设备页面且节点状态为 "stopped"
- **THEN** 页面顶部显示红色网络状态条，下方显示居中的离线空状态（WifiOff 图标 + "节点未启动" + "启动节点"按钮）

#### Scenario: Desktop layout unchanged
- **WHEN** 用户在桌面端或平板端（≥768px）访问设备页面
- **THEN** 页面渲染桌面端布局：Toolbar（标题 + "添加设备"按钮）+ 设备网格卡片列表

### Requirement: Mobile inline network status bar
移动端设备页面 SHALL 在内容区顶部显示内联网络状态条，根据节点状态切换样式。

#### Scenario: Online status bar
- **WHEN** 节点状态为 "running"
- **THEN** 显示绿色背景状态条，内容为绿色圆点 + "P2P 节点运行中 · N 台设备在线"文字 + 红色"停止"按钮

#### Scenario: Offline status bar
- **WHEN** 节点状态为 "stopped"
- **THEN** 显示红色背景状态条，内容为红色圆点 + "P2P 节点未启动"文字

#### Scenario: Starting status bar
- **WHEN** 节点状态为 "starting"
- **THEN** 显示黄色/橙色背景状态条，内容为"节点启动中..."文字

#### Scenario: Stop button opens stop confirmation
- **WHEN** 用户点击在线状态条上的"停止"按钮
- **THEN** 打开停止节点确认弹窗（移动端为 Bottom Sheet）

### Requirement: Mobile offline empty state
当节点未启动时，移动端设备页面 SHALL 在状态条下方显示居中的空状态提示，替代空的设备列表。

#### Scenario: Empty state display
- **WHEN** 节点状态为 "stopped" 且断点为 mobile
- **THEN** 显示居中布局：80px 圆形灰色背景 + WifiOff 图标 + "节点未启动"标题（18px/600）+ "启动 P2P 节点后才能发现设备和传输文件"描述 + "启动节点"按钮（蓝色，200px 宽）

#### Scenario: Start button opens start confirmation
- **WHEN** 用户点击空状态中的"启动节点"按钮
- **THEN** 打开启动节点确认弹窗（移动端为 Bottom Sheet）

### Requirement: Mobile device card list variant
设备卡片在移动端 SHALL 使用横向列表样式（variant="list"），全宽显示。

#### Scenario: Paired device list item
- **WHEN** 渲染已配对设备的移动端列表项
- **THEN** 显示：44px 圆形图标头像（设备类型图标 + 蓝色/灰色背景）+ 设备名称（15px/500）+ 在线状态（绿色圆点 + "在线"，或无状态标识）+ 圆形发送按钮（40px，蓝色背景 + 白色 Send 图标）

#### Scenario: Nearby device list item
- **WHEN** 渲染附近未配对设备的移动端列表项
- **THEN** 显示：44px 圆形图标头像（灰色背景）+ 设备名称 + "未配对"状态文字 + 蓝色描边"连接"按钮

#### Scenario: Desktop card variant unchanged
- **WHEN** 渲染桌面端设备卡片（variant="card" 或默认）
- **THEN** 使用纵向卡片样式（220px 宽），Icon + Name 上方，Action 按钮下方

### Requirement: Mobile bottom navigation 3 tabs
移动端底部导航 SHALL 显示 3 个 tab：设备（Smartphone 图标）/ 传输（Send 图标）/ 设置（Settings 图标）。

#### Scenario: Mobile bottom nav renders 3 tabs
- **WHEN** 在移动端显示底部导航
- **THEN** 渲染 3 个 tab 项，分别为"设备"（/devices）、"传输"（/send）、"设置"（/settings），使用设计稿指定的图标

#### Scenario: Active tab highlight
- **WHEN** 用户当前在某个 tab 对应的路由
- **THEN** 该 tab 的图标和文字显示蓝色高亮（#2563EB）

#### Scenario: Desktop sidebar unchanged
- **WHEN** 在桌面端/平板端显示侧边栏
- **THEN** 侧边栏导航保持 4 项：设备 / 发送文件 / 接收文件 / 设置

### Requirement: Mobile header with add device button
移动端 App 布局（`_app.tsx`）的 Header SHALL 显示 "SwarmDrop" 大标题（24px/700）+ 右侧蓝色圆形 ➕ 按钮（36px 直径）。

#### Scenario: Header display
- **WHEN** 在移动端显示 App 布局 Header
- **THEN** 左侧显示 "SwarmDrop" 文字，右侧显示 36px 蓝色圆形按钮，内含白色 Plus 图标

#### Scenario: Add button triggers pairing menu
- **WHEN** 用户点击移动端 Header 的 ➕ 按钮
- **THEN** 打开添加设备选项（生成配对码 / 输入配对码）
