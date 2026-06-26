## 1. 基础设施与数据层

- [x] 1.1 `network-store` 添加 `startedAt: number | null` 字段，在 `startNetwork` 成功时记录时间戳，`stopNetwork` 时清除
- [x] 1.2 创建 `formatUptime(startedAt: number)` 工具函数，输出"X 小时 Y 分钟"格式（不足 1 小时显示"Y 分钟"，不足 1 分钟显示"刚刚启动"）
- [x] 1.3 拆分导航配置：`nav-items.ts` 导出 `mobileNavItems`（3项：设备/传输/设置）和 `desktopNavItems`（4项：设备/发送文件/接收文件/设置）

## 2. 移动端底部导航

- [x] 2.1 修改 `bottom-nav.tsx` 使用 `mobileNavItems`（3 Tab），更新图标（Smartphone / Send / Settings）
- [x] 2.2 验证 tab 高亮逻辑在 3 tab 配置下正常工作

## 3. 移动端网络状态条

- [x] 3.1 创建 `NetworkStatusBar` 组件（`src/components/network/network-status-bar.tsx`），实现在线（绿色）/离线（红色）/启动中（黄色）三种状态样式
- [x] 3.2 在线状态显示"● P2P 节点运行中 · N 台设备在线" + 红色"停止"按钮
- [x] 3.3 "停止"按钮点击事件连接到 StopNode 弹窗的 open 状态

## 4. 移动端离线空状态

- [x] 4.1 创建 `OfflineEmptyState` 组件（`src/components/network/offline-empty-state.tsx`），包含 WifiOff 图标圆形背景 + 标题 + 描述 + 启动按钮
- [x] 4.2 "启动节点"按钮点击事件连接到 StartNode 弹窗的 open 状态

## 5. 移动端设备卡片列表变体

- [x] 5.1 给 `DeviceCard` 添加 `variant` prop（"card" | "list"），默认为 "card"
- [x] 5.2 实现 `variant="list"` 样式：full-width 横向布局，44px 圆形图标头像 + 设备名/状态 + 操作按钮
- [x] 5.3 已配对设备列表项：蓝色头像背景（在线）/灰色（离线）+ 圆形发送按钮（40px 蓝色）
- [x] 5.4 附近设备列表项：灰色头像背景 + 蓝色描边"连接"文字按钮

## 6. 节点控制响应式弹窗

- [x] 6.1 创建 `StartNodeSheet` 组件（`src/components/network/start-node-sheet.tsx`），使用 `ResponsiveDialog` 包裹
- [x] 6.2 移动端内容：Play 图标 + 标题 + 说明 + 3 项功能列表（Globe/Radar/Shield 图标）+ 启动/取消按钮
- [x] 6.3 桌面端内容：复用现有 NetworkDialog 的离线布局（状态行 + 地址 + 统计 + 启动按钮）
- [x] 6.4 创建 `StopNodeSheet` 组件（`src/components/network/stop-node-sheet.tsx`），使用 `ResponsiveDialog` 包裹
- [x] 6.5 移动端内容：Power 图标 + 标题 + 说明 + 节点信息卡片（Peer ID / 运行时长 / 已连接设备）+ 警告文字 + 停止/取消按钮
- [x] 6.6 桌面端内容：复用现有 NetworkDialog 的运行布局（状态行 + 地址列表 + 统计 + 停止按钮）
- [x] 6.7 确认/取消按钮调用 `startNetwork()` / `stopNetwork()` 并关闭弹窗

## 7. 设备页面重构

- [x] 7.1 在 `devices.lazy.tsx` 中提取共享数据逻辑到顶层 hooks（peers、pairedDevices、nearbyDevices）
- [x] 7.2 创建 `MobileDevicesView` 子组件，渲染移动端布局：NetworkStatusBar + 在线内容（设备列表）或离线内容（空状态）
- [x] 7.3 创建 `DesktopDevicesView` 子组件，渲染桌面端布局：Toolbar + 设备网格卡片（保持现有结构，微调样式）
- [x] 7.4 在 `DevicesPage` 中根据 `useBreakpoint()` 条件渲染 Mobile/Desktop 视图

## 8. 移动端 App 布局 Header

- [x] 8.1 在 `_app.tsx` 移动端分支中添加 Header 区域：左侧 "SwarmDrop" 标题（24px/700）+ 右侧蓝色圆形 ➕ 按钮（36px）
- [x] 8.2 ➕ 按钮点击触发添加设备选项（复用 AddDeviceMenu 逻辑）

## 9. 入口整合与清理

- [x] 9.1 更新侧边栏底部网络状态点击事件，打开新的节点控制弹窗（替代原 NetworkDialog）
- [x] 9.2 确保 GenerateCodeDialog / InputCodeDialog 在新布局下正常工作
- [x] 9.3 运行 `pnpm i18n:extract` 提取新增的翻译字符串
- [x] 9.4 验证移动端和桌面端响应式切换无布局闪烁

## 10. 构建验证

- [x] 10.1 运行 `pnpm build` 确认无 TypeScript 编译错误
- [x] 10.2 运行 `cargo clippy` 确认 Rust 端无警告（如果修改了 store 相关 command）— 本次无 Rust 修改，跳过
