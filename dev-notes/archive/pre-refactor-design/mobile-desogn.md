1. Tauri 窗口最小尺寸
   文件: src-tauri/tauri.conf.json

在 windows 配置中添加 minWidth: 360, minHeight: 480。

2. 新增 useBreakpoint hook
   文件: src/hooks/use-breakpoint.ts（新建）

提供多断点检测能力，返回当前断点 "mobile" | "tablet" | "desktop"：

mobile: <768px
tablet: 768-1023px
desktop: ≥1024px
保留现有 use-mobile.ts 不动（shadcn sidebar 内部依赖它）。

3. 修改侧边栏为可折叠
   文件: src/components/layout/sidebar.tsx

collapsible="none" → collapsible="icon"
为每个 SidebarMenuButton 添加 tooltip prop（图标模式下 hover 显示文字）
Header 中的 "SwarmDrop" 文字在 icon 模式下隐藏，只保留 logo 图标
Footer 在 icon 模式下简化（只显示头像，隐藏文字和状态）
shadcn/ui Sidebar 内置了 icon 模式的所有 CSS：

group-data-[collapsible=icon]:w-(--sidebar-width-icon) 自动缩为 48px
SidebarMenuButton 自动变为 size-8! p-2! 正方形
tooltip 仅在 collapsed + 非移动端时显示 4. 修改 App 布局 - 根据断点切换模式
文件: src/routes/\_app.tsx

使用 useBreakpoint 判断当前断点
desktop (≥1024px): 侧边栏展开，open={true}
tablet (768-1023px): 侧边栏折叠为图标模式，open={false}
mobile (<768px): 隐藏侧边栏，显示底部导航栏
通过 SidebarProvider 的 open 和 onOpenChange 控制：

<SidebarProvider open={!isTablet} onOpenChange={...}>
mobile 模式下整个 sidebar 不渲染，改为底部导航。

5. 新增底部导航栏组件
   文件: src/components/layout/bottom-nav.tsx（新建）

移动端 (<768px) 时显示在页面底部的 Tab 导航栏：

4 个 Tab：设备、发送、接收、设置
固定在底部，高度约 56px
当前路由高亮（蓝色图标+文字）
复用 sidebar.tsx 中的 navItems 数组（需提取为共享常量）6. 提取共享导航配置
文件: src/components/layout/nav-items.ts（新建）

将 navItems 数组从 sidebar.tsx 提取到独立文件，供侧边栏和底部导航栏共用。

7. 修改 App 布局结构
   文件: src/routes/\_app.tsx

移动端时的布局结构变为：

<div class="h-svh flex flex-col">
  <main class="flex-1 overflow-hidden">
    <Outlet />
  </main>
  <BottomNav />  <!-- 仅 mobile 时渲染 -->
</div>
桌面/平板时保持现有的 SidebarProvider 结构。

8. 页面 padding 和工具栏响应式
   文件: src/routes/\_app/devices.lazy.tsx, src/routes/\_app/settings.lazy.tsx

内容区 padding: p-4 md:p-5 lg:p-6
工具栏 padding: px-3 md:px-4 lg:px-5
移动端工具栏增加 SidebarTrigger（当处于 tablet 模式且侧边栏折叠时可点击展开）9. 设备网格断点调整
文件: src/routes/\_app/devices.lazy.tsx

当前: grid-cols-1 sm:grid-cols-2 lg:grid-cols-3
调整为: grid-cols-1 sm:grid-cols-2 xl:grid-cols-3

因为 tablet 模式（768-1024px）下内容区变宽了（只有 48px 侧边栏），2 列网格刚好。3 列在 xl(1280px) 以上才启用。

10. 设置页面响应式微调
    文件: src/routes/\_app/settings.lazy.tsx

Select 宽度在小屏上改为 w-[120px] sm:w-[140px]
设备信息卡片在超小屏上可能需要调整
实现顺序
tauri.conf.json - 设最小窗口尺寸
use-breakpoint.ts - 新建多断点 hook
nav-items.ts - 提取共享导航配置
sidebar.tsx - 改为 collapsible="icon" + tooltip
\_app.tsx - 断点驱动的布局切换
bottom-nav.tsx - 新建底部导航栏
devices.lazy.tsx / settings.lazy.tsx - padding 和网格调整
