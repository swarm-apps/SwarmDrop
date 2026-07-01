## Context

当前配对码 UI 使用 `ResponsiveDialog`（桌面 Dialog / 移动端 Drawer）实现 `GenerateCodeDialog` 和 `InputCodeDialog`。设计稿要求：

- **移动端**：配对流程为**全屏页面**，带 Tab 切换（"生成配对码" / "输入配对码"），不是底部弹窗
- **桌面端生成码**：保持 **Dialog 弹窗**，但布局调整（标题"添加新设备"，按钮"取消/复制配对码"）
- **桌面端输入码**：改为**全屏页面**（内嵌在主内容区），带 `← 连接已有设备` 返回导航

现有实现文件：
- `src/components/pairing/generate-code-dialog.tsx` — ResponsiveDialog 实现
- `src/components/pairing/input-code-dialog.tsx` — ResponsiveDialog 实现
- `src/components/pairing/add-device-menu.tsx` — 下拉菜单入口
- `src/stores/pairing-store.ts` — 状态机管理配对流程

## Goals / Non-Goals

**Goals:**
- 移动端配对 UI 改为全屏页面 + Tab 切换
- 桌面端输入码改为全屏页面（替代 Dialog）
- 桌面端生成码保持 Dialog 但对齐设计稿布局
- 保持现有 pairing-store 状态机逻辑不变
- 保持 `ConnectionRequestDialog` 不变（接收配对请求的弹窗）

**Non-Goals:**
- 不修改配对业务逻辑（Rust 后端 / store actions）
- 不修改 Phase 3 文件传输相关设计（hzztj / F21pf 设计稿属于未来阶段）
- 不引入新路由（避免路由复杂度，使用页面内状态切换）

## Decisions

### D1: 移动端使用页面内状态切换而非路由导航

**选择**：在 `devices.lazy.tsx` 的 `MobileDevicesView` 中通过状态（`pairingView: null | "generate" | "input"`）切换到全屏配对视图，而非新增 TanStack Router 路由。

**理由**：
- 配对视图是临时 overlay，不是独立页面，不需要持久 URL
- 避免路由层引入配对状态管理的复杂度
- 与当前 pairing-store 的 phase 状态机更好配合（store 控制内容，组件控制视图模式）
- 底部导航栏在配对视图中隐藏，全屏视图自带 header

**替代方案**：新增 `/_app/pairing.lazy.tsx` 路由 — 但会增加路由守卫复杂度和导航状态同步问题

### D2: 桌面端输入码使用状态切换渲染全屏内容页

**选择**：在 `devices.lazy.tsx` 的 `DesktopDevicesView` 中通过状态切换到输入码全屏视图，toolbar 显示 `← 连接已有设备`。

**理由**：
- 设计稿显示桌面端输入码页面仍在 sidebar 旁的主内容区，不是独立页面
- 与移动端方案一致，减少架构差异

### D3: 桌面端生成码保持 Dialog 方案

**选择**：继续使用 Dialog，但重写布局对齐设计稿。

**理由**：设计稿明确为弹窗 overlay。

### D4: 移动端 Tab 切换联动 pairing-store

**选择**：Tab 切换时调用对应的 store action（`generateCode()` / `openInput()`），store 的 phase 变化驱动内容渲染。

**理由**：
- 保持 store 作为单一状态源
- Tab 切换 = 功能切换，需要触发对应的初始化逻辑（如生成码需要调用 Rust API）

### D5: 移动端配对视图入口

**选择**：移动端 MobileHeader 的 ＋ 按钮和设备页的"连接"按钮不再打开 DropdownMenu，而是直接进入全屏配对视图（默认显示"生成配对码" Tab）。

**理由**：设计稿中移动端没有下拉菜单，＋ 按钮直接进入配对页面。桌面端保持 DropdownMenu 入口。

## Risks / Trade-offs

- **[风险] 移动端全屏视图与底部导航栏冲突** → 配对视图中隐藏底部导航栏（在 `_app.tsx` 层通过状态控制，或在 devices 页面直接用绝对定位覆盖）
- **[风险] Tab 切换时的状态清理** → 切换 Tab 时调用 `reset()` 清除前一个 Tab 的 store 状态，避免残留数据
- **[权衡] 不使用路由** → 浏览器后退按钮不会返回配对视图的上一步，但配对流程本身是线性的，自带返回按钮足够
