# Theme & Styling

## 概览

桌面端 UI 层的项目特有约束：shadcn/ui new-york 风格、Tailwind v4 token、macOS 自定义标题栏、Aurora 背景。本主题只记录"看代码看不出来 / 容易踩坑"的部分，常规 utility 用法直接查 `/tailwind-css-patterns` 或 `/tailwind-design-system`。

## 窗口装饰 (macOS)

### macOS 标题栏走 Overlay 模式

`src-tauri/tauri.conf.json` 里 `titleBarStyle: "Overlay"` + `hiddenTitle: true` + `trafficLightPosition: { x: 12, y: 22 }`。这意味着：

- 顶栏组件需要给左侧留约 70-80px 给红绿灯占位（仅 macOS）
- 自定义顶栏可以横跨整个窗口宽度，不要尝试绘制原生标题
- Windows 端不走 Overlay，顶栏布局要兼容两种平台（用 `cfg!()` 等价的前端判断 + Tauri OS plugin）

**相关文件**：`src-tauri/tauri.conf.json`、`src/components/layout/*`

## i18n / 翻译

### 源 locale 是 zh（不是 en）

Lingui 配置 `sourceLocale: "zh"`，意味着代码里写 `<Trans>添加设备</Trans>` 是源，翻译目标是 zh-TW / en。

**正确做法**：
- 写新组件时 `<Trans>` 内直接用中文，由 `pnpm i18n:extract` 提取
- t 标签用法：`` t`配对码已过期` ``

**不要做**：
- 用英文做源然后等翻译——会和现有 catalog 风格不一致

**相关文件**：`lingui.config.ts`、`src/locales/{zh,zh-TW,en}/messages.po`

### 实际只有 3 个 locale

CLAUDE.md 顶部写"8 locales (zh, zh-TW, en, ja, ko, es, fr, de)"——这是规划目标，**当前实际只有 zh / zh-TW / en**（见 `lingui.config.ts` 与 `src/locales/`）。加新语言前先确认 Aurora / 字体 fallback 等下游是否准备好。

## Zustand selector 与派生数组

### filter / map 派生值必须套 useShallow

Zustand 默认用 `Object.is` 比较 selector 返回值。`s.devices.filter(...)` 每次返回新数组引用 → 组件无限 re-render（"Maximum update depth exceeded"）。

**正确做法**：
```tsx
import { useShallow } from "zustand/react/shallow";

const nearbyDevices = useNetworkStore(
  useShallow((s) => s.devices.filter((d) => !d.isPaired && d.status === "online")),
);
```

**相关文件**：`src/routes/_app/devices/-components/add-device-menu.tsx`、项目里其他 selectors 见 `src/stores/`

## 暗色主题背景

### 主应用使用全局 app-shell 环境光背景

桌面主应用不要给每个页面单独写大面积深灰或白色背景。统一在 `_app` layout 的 `app-shell` 上使用 `--app-shell-background` 作为纯色兜底底色，并挂载 `AppAmbientBackground` 作为全局动态环境光层。动态层基于 React Bits `SoftAurora` + `SideRays` 的 WebGL / OGL 实现：SoftAurora 提供柔和底层氛围，`AppAmbientLightOverlay` 在暗色主题叠加前景角落光束；失败时降级为纯色背景。

**正确做法**：
- `_app` 根容器保留 `app-shell`
- 全局 `AppTopBar` 使用半透明毛玻璃背景，透出 `app-shell` 环境光；保留原高度和窗口控制布局
- 主应用页面如果是整页背景，使用 `bg-transparent` 透出全局背景
- 全局滚动条 track 保持透明，只用半透明 thumb，避免右侧滚动槽截断环境光
- 主应用高层级面板优先使用全局玻璃态工具类，弹窗 / 表单等 shadcn 基础组件仍使用语义色；`glass-panel` 与设置页常用的 `glass-card` 保持同一填充亮度，区别主要来自边界、阴影和布局角色
- 亮色主题保持白色产品底和白色玻璃材质，SoftAurora 复用暗色主题的蓝 / 青环境光参数，但不叠前景 SideRays；不要把整套面板材质染成蓝绿色
- 背景动效只放在 `app-shell` 的全局环境层；`AppAmbientBackground` 负责底层 SoftAurora，`AppAmbientLightOverlay` 负责暗色主题的 React Bits SideRays 角落光束，`app-shell` 不再叠加额外 CSS radial / beam 渐变；内容卡片不要各自动画
- SideRays 需要更高层级时，优先用独立 `pointer-events: none` overlay + shader 参数控制强度，不要给整层设置 `opacity`，否则 WebGL 光束会先被整体压淡，玻璃卡片下会显得没有效果
- 背景动画必须支持 `prefers-reduced-motion: reduce`
- React Bits 背景只引入 `ogl` 这类轻量 WebGL 依赖；不要为了主应用背景引入 three / react-three-fiber 级别的 3D 栈，除非页面本身需要 3D 场景

**不要做**：
- 顶栏使用整条 `bg-background` / `dark:bg-zinc-950` 实心色块，会把应用视觉切成上下两段
- 给滚动容器或滚动条轨道设置实色背景；这会让滚动槽看起来不透光
- 在单个页面里硬编码 `dark:bg-zinc-950` / `dark:bg-zinc-900` 作为整页背景
- 给 `app-shell` 再加 CSS 渐变、伪元素扫光或 vignette；主氛围只交给 React Bits 背景 / overlay 组件
- 在页面内容层叠加亮色主题的 background image，暗色下要用 `dark:bg-none` 清掉
- 使用高饱和、大面积彩色渐变压过内容层级
- 让首页 `glass-panel` 比设置页 `glass-card` 更实或更暗；跨页面主卡片的底色应该一致
- 把动态光效做成高频闪烁或快速扫光；环境光应该是慢速、低对比的漂移
- 用 `filter: blur(...)` 或大面积 backdrop blur 做滚动背景，容易造成桌面 WebView 重绘压力
- 在单个页面重复挂载 React Bits 背景；这会让主题、性能和 reduced motion 行为分裂

**相关文件**：`src/index.css`、`src/components/layout/app-ambient-background.tsx`、`src/routes/_app.tsx`、`src/routes/_app/devices/index.lazy.tsx`

### 玻璃态层级不要做成卡片套卡片

全局玻璃态工具类定义在 `src/index.css`：`glass-panel` 用于页面主面板，`glass-card` 用于真正独立的设备卡 / 列表项，`glass-control` 用于按钮、计数器、配对码输入框这类小控件，`glass-accent` 用于需要轻微强调的功能区。

**正确做法**：
- 首页主分区用单层 `glass-panel`，不要再做外层壳 + 内层面板的双层容器
- `添加设备` 这种组合工具区内部使用无框分组、分隔线和轻底色，避免附近设备 / 空状态 / 配对码全部各自成卡
- 只有可交互的小控件保留细边界；大面积容器主要靠半透明底色、顶部高光和阴影建立层级
- 如果需要强调配对码，使用 `glass-accent` 做单个柔和区域，不要在它内部再堆多个带边框的小卡片

**不要做**：
- 在一个 `glass-panel` 里连续嵌套多个 `glass-card`，再在 `glass-card` 里嵌套 `glass-control`
- 用明显边框区分所有层级；这会在暗色主题里显得很灰、很累

**相关文件**：`src/index.css`、`src/routes/_app/devices/index.lazy.tsx`、`src/routes/_app/devices/-components/device-card.tsx`
