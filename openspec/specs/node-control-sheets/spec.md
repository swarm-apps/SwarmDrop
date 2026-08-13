# node-control-sheets Specification

## Purpose
TBD - created by archiving change mobile-home-pages. Update Purpose after archive.
## Requirements
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

节点状态面 SHALL 可从常驻状态位打开，且该入口 SHALL 在每一条路由上都在场。

各端载体（分叉是有意的，三端导航形态不同）：

- **桌面**：`AppTopBar` 的状态 pill。桌面端**没有侧边栏**——全局导航是顶栏 + 面包屑，这是既定的刻意简化。
- **移动**：主屏 AppHeader 的状态 pill。
- **Web**：常驻侧栏底部（宽屏）与顶栏右侧（窄屏）的状态 pill。

状态 pill SHALL 具备可访问名，说明它可点开查看详情，SHALL NOT 只暴露状态词。

#### Scenario: 桌面任意路由可达

- **WHEN** 用户在桌面端处于任意一条应用路由
- **THEN** 顶栏状态 pill 在场且可点开节点状态面

#### Scenario: 状态入口有可访问名

- **WHEN** 读屏用户聚焦到状态 pill
- **THEN** 可访问名同时说明当前状态与「点开查看详情」

### Requirement: 节点状态面是单一面，动作随状态切换

节点状态面 SHALL 是**一个组件**，其动作区按当前生命周期状态在「启动节点」与「停止节点」之间切换。

SHALL NOT 拆成两个各自独立的组件——拆开后停机态那一屏无法读取真实状态，只能摆硬编码占位值（当前桌面 `StartNodeSheet` 的节点状态、监听地址与两格计数全部是写死的）。

节点状态面 SHALL NOT 以破坏性动作作为其主要目的：它的主要目的是回答「我现在的网络状况如何」，启停只是其中一个动作。

#### Scenario: 停机态也展示真实状态

- **WHEN** 节点未运行时打开节点状态面
- **THEN** 展示的节点状态、地址与计数来自真实数据源，而非硬编码占位值

#### Scenario: 动作随状态切换

- **WHEN** 节点处于 `running`
- **THEN** 动作区提供「停止节点」；否则提供「启动节点」

### Requirement: 信息位不得因视口尺寸被丢弃

节点状态面 SHALL NOT 使用视口尺寸作为信息披露的开关。空间不足时 SHALL 折叠或内滚。

当前桌面实现以 `window.innerHeight >= 700` 门控七个信息块（两格统计、中继、引导节点、局域网协助、候选来源、公网地址、监听地址），矮窗口下这些信息静默消失且无任何入口找回。

#### Scenario: 矮窗口信息可达

- **WHEN** 桌面窗口高度小于 700px 时打开节点状态面
- **THEN** 全部信息位仍可经折叠或滚动到达

### Requirement: 地址与标识符要么可复制要么不像可点

节点状态面中展示的节点 ID、可达地址、监听地址、失败原因 SHALL 可复制。截断显示时，复制内容与悬停提示 SHALL 是完整值。

#### Scenario: 节点 ID 可复制

- **WHEN** 用户点击节点状态面中截断显示的节点 ID
- **THEN** 完整值被复制到剪贴板并给出反馈

#### Scenario: 失败原因可复制

- **WHEN** 某条基础设施链路展示失败原因
- **THEN** 该字符串可整段复制

### Requirement: 停止节点前检查在途传输

停止或重启节点前，系统 SHALL 检查是否存在在途传输，并在存在时明确告知用户该操作会中断它们。

当前实现直接调用停止流程且无任何检查，确认文案也只说「断开所有连接、其他设备将无法发现你」，不提正在传输的文件会中断。本变更增加了新的重启触发点，因此该防护 SHALL 先于那些触发点落地。

#### Scenario: 有在途传输时告知后果

- **WHEN** 存在活跃传输会话且用户触发停止或重启节点
- **THEN** 确认界面明确说明这些传输会被中断，并给出会话数量

