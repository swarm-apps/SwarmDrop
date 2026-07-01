## Context

设备首页（`/devices`）是 SwarmDrop 的主页面，当前桌面端和移动端共用一套 Toolbar + 网格卡片布局。设计稿提供了两套完整的 UI 方案：桌面端保持 Sidebar + 主内容区的经典布局，移动端采用全新的 Header + 内联状态条 + 列表卡片布局。

**现有架构基础：**
- `useBreakpoint()` hook 提供 3 级断点（mobile/tablet/desktop）
- `_app.tsx` 已在布局层做了 mobile vs sidebar 的条件渲染
- `ResponsiveDialog` 组件已实现 Dialog/Drawer 的自动切换
- `network-store` 提供完整的节点状态和 peer 信息
- Vaul (Drawer) 库已安装

## Goals / Non-Goals

**Goals:**
- 按设计稿实现设备首页的响应式双布局（移动端 + 桌面端）
- 移动端：内联网络状态条、离线空状态、列表式设备卡片、3 Tab 底部导航
- 节点控制弹窗：移动端 Bottom Sheet / 桌面端 Dialog 的响应式切换
- 共享同一份数据逻辑，仅 UI 层按断点差异渲染

**Non-Goals:**
- 不涉及"传输"路由页面的实现（仅调整底部导航 tab 名称和路由指向）
- 不涉及配对流程的修改（GenerateCodeDialog / InputCodeDialog 保持不变）
- 不涉及 Tablet 断点的特殊处理（Tablet 跟随桌面端布局）
- 不涉及暗色模式适配（设计稿当前仅提供 Light 主题）

## Decisions

### D1: 设备页面内部断点分支 vs 拆分为独立组件

**选择：在 `devices.lazy.tsx` 内部按断点条件渲染两个子组件**

- 创建 `MobileDevicesView` 和 `DesktopDevicesView` 两个内部组件
- 共享同一份数据逻辑（hooks 提到顶层），通过 props 传递
- 避免了重复路由注册和数据获取逻辑

替代方案：拆分为 `devices.mobile.tsx` + `devices.desktop.tsx` 两个独立文件，在路由层根据断点懒加载。更彻底地隔离，但增加了路由配置复杂度，且 TanStack Router 的 file-based routing 不原生支持这种模式。

### D2: 移动端底部导航 3 Tab vs 保持 4 Tab

**选择：移动端 3 Tab，桌面端保持 4 项侧边栏导航**

- 设计稿明确定义移动端为 3 Tab：设备 / 传输 / 设置
- 导航配置改为两套：`mobileNavItems`（3项）和 `desktopNavItems`（4项）
- "传输" tab 指向 `/transfers`（或保持 `/send`），合并发送和接收
- 桌面端侧边栏保持 4 项不变（设备 / 发送文件 / 接收文件 / 设置）

### D3: 节点控制弹窗复用 ResponsiveDialog vs 独立实现

**选择：复用 ResponsiveDialog 模式，但内容区按端差异渲染**

- 外壳：`ResponsiveDialog` 自动处理 Dialog/Drawer 切换
- 内容：移动端和桌面端显示不同的内容布局
  - 移动端：图标 + 标题 + 说明 + 功能列表/节点信息 + 按钮组（纵向排列）
  - 桌面端：与现有 `NetworkDialog` 布局保持一致（状态行 + 地址列表 + 统计数字 + 单按钮）
- 将现有 `NetworkDialog` 重构为 `NodeControlDialog`，内部按 `useResponsiveDialog()` 的 `isMobile` 切换内容

替代方案：完全独立实现 `StartNodeSheet` 和 `StopNodeSheet`，与 `NetworkDialog` 并存。更简单，但导致启动/停止逻辑分散在多个组件中。

### D4: 移动端设备卡片实现方式

**选择：给 `DeviceCard` 添加 `variant` prop（"card" | "list"）**

- `variant="card"`（默认）：现有纵向卡片样式，用于桌面端
- `variant="list"`：横向列表样式，full-width，用于移动端
- 共享同一份 Device 接口和操作逻辑

替代方案：创建独立的 `DeviceListItem` 组件。更解耦，但 Device 接口和操作逻辑完全相同，拆分收益不大。

### D5: 内联网络状态条的位置

**选择：作为移动端设备页面的子组件，放在内容区顶部**

- `NetworkStatusBar` 组件，仅在移动端渲染
- 在线状态：绿色背景 + 状态文字 + "停止"按钮（点击打开 StopNode Sheet）
- 离线状态：红色背景 + 状态文字
- 从 `network-store` 读取 `status` 和 `getConnectedCount()`

## Risks / Trade-offs

- **[Bottom Nav Breaking Change]** 移动端从 4 tab 改为 3 tab，"/send" 和 "/receive" 路由尚未实现，"/transfers" 路由也不存在 → 先将"传输" tab 指向 `/send` 作为占位，后续 Phase 3 实现传输页面时再调整
- **[桌面端 NetworkDialog 重构]** 将现有 NetworkDialog 重构为响应式组件可能影响从侧边栏打开的入口 → 保持 `open/onOpenChange` API 不变，仅内部实现改变
- **[运行时长数据缺失]** 停止节点 Bottom Sheet 设计稿中显示"运行时长"，但当前 `network-store` 没有 `startedAt` 字段 → 需要在 store 中添加 `startedAt` 时间戳，在 `startNetwork` 成功时记录
- **[Peer ID 显示]** 停止节点 Sheet 需要显示当前节点的 Peer ID，当前 store 中没有存储 → 可从 `secret-store` 的 `deviceId` 获取并截断显示
