## Why

当前配对码 UI（GenerateCodeDialog / InputCodeDialog）使用 ResponsiveDialog（桌面 Dialog / 移动端 Drawer），但设计稿要求完全不同的交互模式：移动端应为**全屏页面**（带 Tab 切换），桌面端输入码也应为**全屏页面**（带返回导航）。现有实现与设计稿严重不符，需要重新实现。

## What Changes

- **BREAKING** 移动端配对 UI 从底部弹窗改为全屏页面，包含 Tab 切换（"生成配对码" / "输入配对码"）
- **BREAKING** 桌面端"输入配对码"从弹窗改为全屏页面，带 `← 连接已有设备` 返回导航
- 桌面端"生成配对码"保持弹窗形式，但调整布局：标题改为"添加新设备"，按钮改为"取消 / 复制配对码"
- 移动端全屏页面包含：Header（← 添加设备 + X 关闭）、Tab 切换栏、内容区域（生成码展示或 OTP 输入）
- 桌面端输入码全屏页面包含：Toolbar（← 连接已有设备）、居中内容（Link 图标 + OTP 输入 + 取消/确认按钮）
- 移动端底部按钮样式：生成码页面"重新生成"（蓝色全宽），输入码页面"查找设备"（蓝色全宽）
- 入口调整：移动端/桌面端"添加设备"菜单点击后导航到对应页面/弹窗，而非直接打开 Drawer

## Capabilities

### New Capabilities
- `pairing-page`: 配对全屏页面 —— 移动端 Tab 全屏视图 + 桌面端输入码全屏页面，替代当前的 ResponsiveDialog 弹窗模式

### Modified Capabilities
（无已有 specs）

## Impact

- `src/components/pairing/generate-code-dialog.tsx` — 重写，桌面端保持 Dialog 但调整布局；移动端逻辑移至新全屏页面
- `src/components/pairing/input-code-dialog.tsx` — 重写，桌面端改为全屏页面；移动端逻辑移至新全屏页面
- `src/routes/_app/` — 可能新增配对相关路由页面，或在设备页面内嵌入全屏视图
- `src/components/pairing/add-device-menu.tsx` — 入口逻辑调整，点击后导航而非触发 store action
- `src/routes/_app.tsx` — MobileHeader 中 ＋ 按钮行为调整
- `src/stores/pairing-store.ts` — 可能需要新增 phase 或调整 phase 流转以适配页面级导航
