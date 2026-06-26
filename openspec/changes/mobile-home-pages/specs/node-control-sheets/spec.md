## ADDED Requirements

### Requirement: Start node confirmation dialog
系统 SHALL 提供启动节点确认弹窗，移动端为 Bottom Sheet（Drawer），桌面端为居中 Dialog。

#### Scenario: Mobile start node bottom sheet content
- **WHEN** 在移动端打开启动节点确认弹窗
- **THEN** 从底部弹出 Drawer，显示以下内容（从上到下）：拖动手柄 → 蓝色圆形 Play 图标（64px）→ "启动 P2P 节点"标题（18px/600）→ 说明文字"将连接到 SwarmDrop 网络，其他设备将能够发现你并发送文件。" → 功能列表（3 项，每项含图标 + 文字）→ 蓝色"启动"按钮（full-width, 48px 高）→ "取消"文字按钮

#### Scenario: Mobile start node feature list
- **WHEN** 显示启动节点 Bottom Sheet 的功能列表
- **THEN** 列表包含 3 项：Globe 图标 + "连接到 DHT 引导节点"、Radar 图标 + "启用局域网设备发现 (mDNS)"、Shield 图标 + "开启 NAT 穿透和中继"，每项之间有分割线

#### Scenario: Desktop start node dialog content
- **WHEN** 在桌面端打开启动节点确认弹窗
- **THEN** 显示居中 Dialog，内容与现有 NetworkDialog 的离线状态布局一致：标题"网络节点" + 描述 + 节点状态（未启动）+ 监听地址（空）+ 统计数字（0/0）+ "启动节点"按钮

#### Scenario: Confirm start triggers network startup
- **WHEN** 用户在启动确认弹窗中点击"启动"按钮
- **THEN** 调用 `network-store.startNetwork()`，弹窗关闭

#### Scenario: Cancel dismisses sheet
- **WHEN** 用户在启动确认弹窗中点击"取消"或下拉关闭
- **THEN** 弹窗关闭，不执行任何操作

### Requirement: Stop node confirmation dialog
系统 SHALL 提供停止节点确认弹窗，移动端为 Bottom Sheet（Drawer），桌面端为居中 Dialog。

#### Scenario: Mobile stop node bottom sheet content
- **WHEN** 在移动端打开停止节点确认弹窗
- **THEN** 从底部弹出 Drawer，显示以下内容（从上到下）：拖动手柄 → 红色圆形 Power 图标（64px，红色背景 #FEE2E2）→ "停止 P2P 节点"标题（18px/600）→ 说明文字"停止后将断开所有连接，其他设备将无法发现你。" → 节点信息卡片（红色背景 #FEF2F2）→ 红色警告文字"所有活跃连接将被断开" → 红色"停止节点"按钮（full-width, 48px）→ "取消"文字按钮

#### Scenario: Mobile stop node info card
- **WHEN** 显示停止节点 Bottom Sheet 的节点信息卡片
- **THEN** 卡片包含 3 行信息，每行左侧为标签、右侧为值：Peer ID（截断显示，如 "12D3K...bMASX"）/ 运行时长（格式化为"X 小时 Y 分钟"）/ 已连接设备（"N 台"），行间有 border-top 分割线

#### Scenario: Desktop stop node dialog content
- **WHEN** 在桌面端打开停止节点确认弹窗
- **THEN** 显示居中 Dialog，内容与现有 NetworkDialog 的运行状态布局一致：标题"网络节点" + 描述 + 节点状态（运行中）+ 监听地址列表 + 统计数字 + "停止节点"按钮（destructive 样式）

#### Scenario: Confirm stop triggers network shutdown
- **WHEN** 用户在停止确认弹窗中点击"停止节点"按钮
- **THEN** 调用 `network-store.stopNetwork()`，弹窗关闭

#### Scenario: Cancel dismisses sheet
- **WHEN** 用户在停止确认弹窗中点击"取消"或下拉关闭
- **THEN** 弹窗关闭，不执行任何操作

### Requirement: Node uptime tracking
`network-store` SHALL 记录节点启动时间，以支持停止节点弹窗中"运行时长"的显示。

#### Scenario: Record start time on network start
- **WHEN** `startNetwork()` 成功且节点状态变为 "running"
- **THEN** store 中记录 `startedAt` 为当前时间戳（Date.now()）

#### Scenario: Clear start time on network stop
- **WHEN** `stopNetwork()` 执行后节点状态变为 "stopped"
- **THEN** store 中清除 `startedAt`（设为 null）

#### Scenario: Format uptime display
- **WHEN** 停止节点弹窗需要显示运行时长
- **THEN** 计算 `Date.now() - startedAt`，格式化为"X 小时 Y 分钟"（不足 1 小时显示"Y 分钟"，不足 1 分钟显示"刚刚启动"）

### Requirement: Responsive node control dialog entry points
节点控制弹窗 SHALL 可从多个入口触发打开。

#### Scenario: Open from mobile status bar stop button
- **WHEN** 用户点击移动端网络状态条上的"停止"按钮
- **THEN** 打开停止节点确认弹窗

#### Scenario: Open from mobile empty state start button
- **WHEN** 用户点击移动端离线空状态中的"启动节点"按钮
- **THEN** 打开启动节点确认弹窗

#### Scenario: Open from sidebar network status (desktop)
- **WHEN** 用户在桌面端点击侧边栏底部的网络状态区域
- **THEN** 打开节点控制 Dialog（根据当前节点状态显示启动或停止内容）
