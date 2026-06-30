## 1. 基础组件与状态层准备

- [x] 1.1 安装 shadcn/ui Tabs 组件（`pnpm dlx shadcn@latest add tabs`），确认 `src/components/ui/tabs.tsx` 生成
- [x] 1.2 `pairing-store` 新增 `view` 字段（`'none' | 'mobile-pairing' | 'desktop-input'`），用于控制页面级全屏展示；新增 `openMobilePairing()` / `openDesktopInput()` / `closePairingView()` action

## 2. 移动端配对全屏页面

- [x] 2.1 创建 `MobilePairingPage` 组件（`src/components/pairing/mobile-pairing-page.tsx`）：全屏容器（fixed/absolute 覆盖）+ Header（← 添加设备 18px/600 + X 关闭按钮）+ Tabs 切换栏（圆角灰色背景 bg-muted，活跃白色+阴影）
- [x] 2.2 创建 `MobileGenerateCodeView` 组件（`src/components/pairing/mobile-generate-code-view.tsx`）：Link 图标（蓝色 64px 圆形背景 blue/8%）+ 说明文字 14px muted + 6 位数字展示（3-分隔-3，48x60 圆角方块 bg-muted，28px/700）+ 倒计时（Clock 图标 + 过期文字）+ 蓝色全宽按钮"重新生成"（RefreshCw 图标）
- [x] 2.3 创建 `MobileInputCodeView` 组件（`src/components/pairing/mobile-input-code-view.tsx`）：Keyboard 图标（蓝色 64px 圆形背景 blue/8%）+ 说明文字 14px muted + 6 位 OTP 输入框（3-分隔-3，44x56，蓝色/灰色边框）+ 蓝色全宽按钮"查找设备"（Search 图标）
- [x] 2.4 在 `_app.tsx` 移动端分支集成 `MobilePairingPage`：当 `view === 'mobile-pairing'` 时覆盖 main + BottomNav

## 3. 桌面端配对 UI

- [x] 3.1 重写 `generate-code-dialog.tsx` 桌面端布局：Dialog 400px 宽，Link 图标 64px 圆形背景 blue-50 + 标题"添加新设备" 20px/600 + 描述 14px muted + 6 位数字（48x56 bg-muted，24px/600）+ 过期提示 + 底部按钮"取消"（outline）/"复制配对码"（蓝色 Copy 图标，点击后"已复制" Check 图标 2 秒恢复）
- [x] 3.2 创建 `DesktopInputCodePage` 组件（`src/components/pairing/desktop-input-code-page.tsx`）：Toolbar 区域显示 `← 连接已有设备`（带返回箭头）+ 居中内容：Link 图标 64px 圆形背景 blue-50 + 标题"连接已有设备" 20px/600 + 描述 14px muted + 6 位 OTP 输入框（48x56）+ 底部按钮"取消"（outline）/"确认"（蓝色）
- [x] 3.3 在 `_app.tsx` 桌面端分支集成 `DesktopInputCodePage`：当 `view === 'desktop-input'` 时替代 Outlet 显示

## 4. 入口行为调整

- [x] 4.1 移动端 `MobileHeader`：＋ 按钮点击直接调用 `openMobilePairing()` 进入全屏配对页面（移除 DropdownMenu），默认选中"生成配对码" Tab
- [x] 4.2 桌面端 `add-device-menu.tsx`：菜单项"生成配对码"调用 `generateCode()` 打开 Dialog；"输入配对码"调用 `openDesktopInput()` 进入全屏页面

## 5. 清理与验证

- [x] 5.1 删除旧的 `input-code-dialog.tsx`（移动端逻辑已移至 MobileInputCodeView，桌面端已移至 DesktopInputCodePage）
- [x] 5.2 运行 `pnpm i18n:extract` 提取新增翻译字符串
- [x] 5.3 运行 `pnpm build` 确认无 TypeScript 编译错误
