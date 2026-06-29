# Theme & Styling

## 概览

NativeWind 5（preview）+ Tailwind v4 + @rn-primitives/* 组成 UI 层；`tailwind-merge` +
`clsx`（合成的 `cn` helper 在 [src/lib/utils.ts](../../src/lib/utils.ts)）做 className 合并；
图标走 `lucide-react-native`；色彩走 CSS 变量（`src/global.css`）+ `useThemeColors` hook 拉对应
JS 值。主题持久化在 [src/lib/theme-persistence.ts](../../src/lib/theme-persistence.ts)。

## NativeWind / Tailwind

### 拼接 className 时用 cn() 而不是嵌套三元

NativeWind 的 className 字符串能被 tailwind-merge 处理冲突类。两段相同前缀只换一两个类时，
嵌套三元会让代码膨胀且易遗漏共有前缀。

**正确做法**：

```tsx
<View className={cn(
  "size-1.5 rounded-full",
  isOnline ? "bg-success" : "bg-muted-foreground",
)} />
```

**不要做**：

```tsx
<View className={
  isOnline ? "size-1.5 rounded-full bg-success"
           : "size-1.5 rounded-full bg-muted-foreground"
} />
```

**相关文件**：[src/lib/utils.ts](../../src/lib/utils.ts), [src/app/send/select-device.tsx](../../src/app/send/select-device.tsx)

### lightningcss 必须锁 1.30.1

NativeWind 5 preview 依赖的 lightningcss 在新版本有 ABI 不兼容（构建时 panic）。`package.json`
的 `pnpm.overrides` 强制 `lightningcss: 1.30.1`，升级要先验证 nativewind preview 兼容性。

**相关文件**：[package.json](../../package.json)（pnpm.overrides 段）

### lucide-react-native 必须用 `color` prop 上色，className 失效

NativeWind v5 preview 没有为 `lucide-react-native` 注册 `cssInterop`，所以
`<Pause className="text-foreground" />` 这种写法只会把 className 串往下塞，icon 的
`color` prop 还是默认黑色 —— UI 上表现为「黑色描边图标」无论亮色/暗色主题。

**正确做法**：

```tsx
const colors = useThemeColors();
<Pause size={16} color={colors.foreground} />
<CheckCircle2 size={32} color={colors.success} strokeWidth={2} />
```

非主题里的固定色（amber-500、blue-500）直接写 hex：

```tsx
const FOLDER_COLOR = "#f59e0b";  // amber-500
<Folder size={18} color={FOLDER_COLOR} />
```

**变体场景**：把「resolve 出颜色字符串」做成 variant 表的一部分，不要和 className 写法
拆成两套来源：

```tsx
const variantStyles: Record<Variant, { row: string; iconColor: (c: ThemeColors) => string }> = {
  completed: { row: "bg-success/5", iconColor: (c) => c.success },
  ...
};
const styles = variantStyles[variant];
<Icon size={18} color={styles.iconColor(colors)} />
```

**`<Text>` / `<View>` 类内置组件**：NativeWind 已经接管，className `text-foreground` 等正常上色，
**只有 lucide icon 必须显式 `color`**。

**相关文件**：[src/hooks/useThemeColors.ts](../../src/hooks/useThemeColors.ts)，
[src/components/file-tree/file-tree-item.tsx](../../src/components/file-tree/file-tree-item.tsx)
（variantStyles + iconColor resolver 模板）

## 安全区域 / 导航

### 主底部导航使用 NativeTabs，不再用 JS Tabs 自绘高度

主入口 `src/app/(main)/_layout.tsx` 使用 `expo-router/unstable-native-tabs`。底部栏交给系统
tab bar 处理安全区、按压反馈和切换动画，避免 `expo-router` JS `<Tabs>` + 手动
`height/paddingBottom` 造成的动画和布局观感不自然。

**正确做法**：
- Label 传字符串，不能传 `<Trans>`；用 `useLingui().t` 得到翻译后的字符串
- Icon 使用 native tabs 支持的 SF Symbols / Android Material Symbols，例如 `sf="tray"`、`md="inbox"`
- 主题色通过 `backgroundColor`、`iconColor`、`labelStyle`、`shadowColor` 注入
- Android ripple 设为 `transparent`，主底栏不展示 Material 波纹动画

**不要做**：
- 不要在主底栏继续用 `useSafeAreaInsets()` 手动叠加高度和 padding
- 不要把 `lucide-react-native` 组件直接塞进 NativeTabs icon，native tabs 需要系统图标、drawable 或图片源

**相关文件**：`src/app/(main)/_layout.tsx`

### NotifierRoot 用 useRNScreensOverlay 才能浮在 modal 上

`react-native-notifier` 默认渲染层在 Stack 下，bottom-sheet / 全屏 modal 弹起后 toast 会被挡住。
启用 `useRNScreensOverlay` 走 RNScreens 的 overlay 层，跨 modal 显示。

**正确做法**：

```tsx
<NotifierRoot useRNScreensOverlay />
```

**相关文件**：[src/app/_layout.tsx](../../src/app/_layout.tsx)
