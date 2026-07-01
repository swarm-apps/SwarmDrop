## Why

当前设备页面是快速搭建的功能性界面，桌面端和移动端共用同一套布局，缺乏针对不同端的专属设计。设计稿（`dev-notes/design/design.pen`）中已完成桌面端和移动端两套完整的 UI 设计，需要按照设计稿重构设备首页，通过响应式断点切换实现两套不同的页面布局和交互体验。

## What Changes

### 移动端 (<768px) — 全新设计

- **重构移动端设备页面布局**：移除现有 toolbar，改为设计稿的移动端专属布局（"SwarmDrop" 大标题 Header + 蓝色圆形 ➕ 按钮 + 内联网络状态条 + 列表式设备卡片）
- **新增内联网络状态条（仅移动端）**：内容区顶部显示绿色（在线：`● P2P 节点运行中 · N 台设备在线` + 红色"停止"按钮）或红色（离线：`● P2P 节点未启动`）的状态横条
- **新增离线空状态（仅移动端）**：节点未启动时显示居中的 WifiOff 图标 + "节点未启动"标题 + "启动节点"按钮，替代空设备列表
- **移动端设备卡片改为列表样式**：full-width 横向布局（44px 圆形图标头像 + 设备名/在线状态 + 操作按钮），替代现有的纵向网格卡片
- **底部导航从 4 Tab 改为 3 Tab**：设备（Smartphone）/ 传输（Send）/ 设置（Settings），合并发送/接收为"传输" **BREAKING**

### 移动端节点控制 — Bottom Sheet

- **启动节点 Bottom Sheet**：从底部弹出 Drawer，显示 Play 图标 + "启动 P2P 节点"标题 + 说明文字 + 功能列表（DHT 引导 / mDNS 发现 / NAT 穿透）+ 启动/取消按钮
- **停止节点 Bottom Sheet**：从底部弹出 Drawer，显示 Power 图标 + "停止 P2P 节点"标题 + 说明文字 + 节点信息卡片（Peer ID / 运行时长 / 已连接设备数）+ 红色警告文字 + 停止/取消按钮

### 桌面端 (≥768px) — 微调优化

- **保持现有 Sidebar + Toolbar + 内容区布局**，与设计稿基本一致
- **桌面端设备卡片微调**：保持纵向卡片样式（220px），调整为设计稿的布局（Icon+Name Header 上方，Action 按钮下方）
- **"添加设备" 按钮**：蓝色背景 + "添加设备 ▾" 下拉菜单，保持现有 Dropdown 交互
- **启动/停止节点保持 Dialog 模态框**：桌面端继续使用居中 Dialog（与现有 NetworkDialog 布局一致），内容与设计稿对齐

### 响应式适配策略

- 利用现有 `useBreakpoint()` hook 的 3 级断点系统，在设备页面内部按断点条件渲染不同的布局
- 复用已有 `ResponsiveDialog` 模式（移动端 Drawer / 桌面端 Dialog）用于节点控制弹窗
- 共享同一份数据逻辑（network-store、secret-store），仅 UI 层按断点切换

## Capabilities

### New Capabilities
- `home-page-redesign`: 设备首页响应式重构，包括移动端（Header + 内联状态条 + 列表卡片 + 空状态）和桌面端（Toolbar + 网格卡片）两套布局，通过断点切换
- `node-control-sheets`: 启动/停止节点的响应式弹窗（移动端 Bottom Sheet / 桌面端 Dialog），替代现有 NetworkDialog

### Modified Capabilities
（无现有 spec 需要修改）

## Impact

- **前端组件 — 修改**：
  - `src/routes/_app/devices.lazy.tsx` — 设备页面核心重构，按断点渲染两套布局
  - `src/components/devices/device-card.tsx` — 增加移动端列表样式变体
  - `src/components/layout/bottom-nav.tsx` — 3 tab 导航（设备/传输/设置）
  - `src/components/layout/nav-items.ts` — 移动端导航项配置（3项 vs 桌面端4项）
  - `src/components/network/network-dialog.tsx` — 重构为响应式组件（Dialog / Drawer）
  - `src/components/pairing/add-device-menu.tsx` — 移动端使用圆形 ➕ 按钮触发
- **前端组件 — 新增**：
  - `NetworkStatusBar` — 移动端内联网络状态条
  - `OfflineEmptyState` — 移动端节点离线空状态
  - `StartNodeSheet` — 启动节点 Bottom Sheet 内容
  - `StopNodeSheet` — 停止节点 Bottom Sheet 内容
- **依赖**：已有 Vaul（Drawer 库）和 `responsive-dialog.tsx` 模式可直接复用
- **影响范围**：设备首页全部断点，底部导航仅移动端
