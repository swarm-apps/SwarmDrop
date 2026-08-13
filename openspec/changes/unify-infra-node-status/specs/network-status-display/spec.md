## MODIFIED Requirements

### Requirement: NetworkStatus 新增引导节点连接状态

`NetworkStatus` SHALL 维护 `bootstrapConnected: bool`，表示是否至少有一个引导节点已连接。

该字段的口径 SHALL 是「扫描全部已连 peer 的 agent 前缀」，与候选表是不同集合（候选表有学习上限与可用地址两道闸），因此 SHALL NOT 改为从 `infraLinks` 派生。

`bootstrapConnected` SHALL NOT 作为常驻状态位的唯一判据——它在节点启动的握手窗口内必然为 false，直接据此变色会闪；且它不区分「用户关闭了公网可达性」与「连不上」。常驻状态位的判据见「常驻状态位反映健康度而非进程生命周期」。

#### Scenario: 引导节点已连接

- **WHEN** 事件循环检测到与引导节点的连接建立
- **THEN** `bootstrapConnected` 设置为 `true`

#### Scenario: 引导节点断开

- **WHEN** 与全部引导节点的连接均已断开
- **THEN** `bootstrapConnected` 设置为 `false`

### Requirement: 桌面端设备页展示网络状态栏

桌面端 SHALL 在**每一条路由**上常驻展示节点状态入口。该入口的载体是 `AppTopBar` 的状态 pill——桌面端已无侧边栏，全局导航是顶栏 + 面包屑。

SHALL NOT 要求桌面端与移动端共用同一个 React 组件：三端导航形态有意分叉，统一的是信息模型与状态语义。

#### Scenario: 任意路由下都能到达节点状态

- **WHEN** 用户处于任意一条应用路由
- **THEN** 顶栏常驻展示节点状态入口，点击可打开节点状态面

#### Scenario: 离线时展示空状态

- **WHEN** 节点未运行且在设备页
- **THEN** 显示离线空状态并提供启动入口

### Requirement: NetworkStatusBar 展示详细网络状态

常驻状态位 SHALL 展示**一句可达性后果句**，SHALL NOT 展示裸机制布尔的并列（NAT / 公网可达 / 中继 / 引导节点 / 局域网协助等逐项徽标属于诊断层）。

逐条机制状态 SHALL 在诊断层呈现，且 SHALL 使用 `infraLinks` 而非聚合布尔——聚合布尔无法回答「哪一条连不上、为什么」。

状态色 SHALL 走 `success` / `warning` / `destructive` 令牌与其 `-ink` 文本变体，SHALL NOT 直用调色板类名，SHALL NOT 使用 `--info`（该色已被连接类型徽标占用）。

#### Scenario: 可达时的常驻表达

- **WHEN** 本机持有活跃公网可达地址
- **THEN** 常驻位为成功态并说明「跨网络的设备可以连到你」

#### Scenario: 仅局域网时的常驻表达

- **WHEN** 无公网可达但有已连对端
- **THEN** 常驻位为中性态并说明「只有同一网络里的设备能连到你」，不使用警示色

#### Scenario: 完全不可达时的常驻表达

- **WHEN** 节点运行、全部中继失败、无已连对端且已过宽限期
- **THEN** 常驻位为警示态并提供打开诊断层的入口

## ADDED Requirements

### Requirement: 常驻状态位反映健康度而非进程生命周期

系统 SHALL 把节点生命周期与网络健康度表达为两条正交的轴。常驻状态位 SHALL 按 `status !== running ? 生命周期文案 : 健康度文案` 渲染。

同一构建内 SHALL NOT 对同一状态给出两套文案（当前桌面顶栏说「在线 · 可接收」而同一状态在弹窗内是「运行中」）。三端 SHALL 使用同一组状态词。

#### Scenario: 节点在跑但不可达不显示在线

- **WHEN** 节点 `running` 但整体健康度为 `Isolated`
- **THEN** 常驻状态位不呈现「在线」类成功文案

#### Scenario: 同一状态同一文案

- **WHEN** 同一节点状态同时出现在常驻位与节点状态面
- **THEN** 两处使用相同的状态词
