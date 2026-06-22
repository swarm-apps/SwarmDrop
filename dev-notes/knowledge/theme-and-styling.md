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
