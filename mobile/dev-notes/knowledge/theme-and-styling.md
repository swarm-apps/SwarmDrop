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

### 状态语义色当「文字」必须用 `-ink` 变体（State Ink Rule）

`--success` / `--warning` / `--destructive` / `--primary` 这几个饱和色是给**填充 / 圆点 / 图标**
调过的，当**小字**压在浅底或同色 `/10`~`/15` tint 上时对比度全不达 WCAG AA（amber 只有 ~2:1，
green ~3.3:1，red ~3.5:1，blue ~3.4:1）。所以 `global.css` 另有一套高对比文字变体
`--success-ink` / `--warning-ink` / `--destructive-ink` / `--primary-ink`（亮=深一档、暗=亮一档，
均 ≥4.5:1）。

**规则**：状态色作**圆点 / 填充 / 图标** → 用基础 token（`bg-success`、`color={colors.success}`）；
作**文字** → 用 ink 变体（`text-success-ink`、`text-destructive-ink`…）。集中映射处
（`status-pill` TEXT_CLASS、`trust-badge` TRUST_META、`connection-badge` CONNECTION_META、
`transfer/shared` STATUS_META、`file-tree-item` variantStyles）都已遵循；新写状态文案时别再用
`text-destructive` 这类基础色（会渲染成低对比）。完整说明见 DESIGN.md 的「State Ink Rule」。

**相关文件**：[src/global.css](../../src/global.css)，[DESIGN.md](../../DESIGN.md)

### FlatList / SectionList 用 `contentContainerStyle`，不要用 `contentContainerClassName`

NativeWind 5 preview 给 `ScrollView` 注册了 `contentContainerClassName`（`AppScreen` 在用），
但**没有确认为 `FlatList` / `SectionList` 注册**。把可增长列表从 `AppScreen`（ScrollView）切到
虚拟化容器时，内容内边距要走 `contentContainerStyle` 数值，不要赌 `contentContainerClassName`
生效。共享常量 `LIST_CONTENT_PADDING`（[src/components/mobile/screen.tsx](../../src/components/mobile/screen.tsx)）
= AppScreen 的 `px-5 pb-8 pt-1`（20/32/4），inbox + activity 两处虚拟化列表复用它，避免魔数漂移。
列表**内层** View / 行组件的 className 照常生效（NativeWind 只对内置组件的 `contentContainerStyle`
这类间接 prop 有注册盲区）。

**相关文件**：[src/app/(main)/inbox.tsx](../../src/app/(main)/inbox.tsx)（FlatList），
[src/app/activity.tsx](../../src/app/activity.tsx)（SectionList）

### 内容区加载态用骨架屏镜像真实布局,spinner 只留给按钮/行内

内容区(列表初载、详情初载、搜索检索中)的加载态用 `Skeleton`(`src/components/ui/skeleton.tsx`)
拼出与加载完成后**相同结构**的占位——真实卡片 chrome(边框/圆角/内边距)照写,文字/图标位置
放 Skeleton 块;按钮内、输入框内的行内 spinner 仍用 `ActivityIndicator`(蓝底按钮内颜色必须
`colors.primaryForeground`,不要 `colors.background`,那是 Unified Ink Rule 修过的低对比坑)。

**要点**:
- 列表骨架 3-5 行,行间距与真实列表一致;文本行宽度错落(w-1/2 / w-2/3 / w-1/3)避免呆板
- 骨架容器加 `accessible accessibilityLabel={t`加载中`}`;保留原加载容器的 testID
- 初载与"假空态"要区分:`loading && items.length === 0` 走骨架,别闪 EmptyState((main)/inbox 曾踩)
- `animate-pulse` 在原生端可能不动画(NativeWind 5 preview),静态灰块可接受,不要自己写动画
- 静态占位行用 index 作 key 时加 `// biome-ignore lint/suspicious/noArrayIndexKey: 静态占位行,无重排`

**相关文件**:[src/app/transfer/[sessionId].tsx](../../src/app/transfer/[sessionId].tsx)、
[src/app/inbox/[itemId].tsx](../../src/app/inbox/[itemId].tsx)、[src/app/(main)/inbox.tsx](../../src/app/(main)/inbox.tsx)、
[src/app/inbox/search.tsx](../../src/app/inbox/search.tsx)

### 圆角只用 Radius Vocabulary 里的五种语义

按钮=`rounded-xl`;surface/卡/成组容器/输入槽=`rounded-lg`;身份类图标 chip(设备平台/弹窗头/空态)=
`rounded-full`;行首内容类型 chip(收件箱行/文件行/sheet 行)=`rounded-xl`;徽标/pill=`rounded-full`。
不要写字面量圆角(`rounded-[28px]` 这类);完整规则与已知豁免见 DESIGN.md「The Radius Vocabulary Rule」。

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

### SafeAreaView 上的 className `paddingHorizontal` 会被 inset padding 覆盖 → 内容贴边

`react-native-safe-area-context` 的 `<SafeAreaView>` 会把它算出的 safe-area inset 写成自身的
`padding`,**覆盖掉 NativeWind className 里的 `px-*`(左右会变 0,内容贴屏幕边)**。`bg-*` 这类
非 padding 的 className 仍生效,所以现象是"背景色对了、但左右没有内边距"。

**正确做法**:SafeAreaView 只放 `bg-*` + flex + `edges`,把水平/垂直内边距放到**内层 View**。
这也是 `AppScreen`([src/components/mobile/screen.tsx](../../src/components/mobile/screen.tsx))一直
在用的约定。

```tsx
<SafeAreaView style={{ flex: 1 }} className="bg-background" edges={["top", "bottom"]}>
  <View className="flex-1 px-7 pt-4">{children}</View>
  <View className="gap-4 px-7 pb-4 pt-4">{footer}</View>
</SafeAreaView>
```

**不要做**:`<SafeAreaView className="bg-background px-6 py-4">` —— `px-6`/`py-4` 会被 inset
覆盖,onboarding 三屏最初就是这样导致卖点和按钮贴边。

**相关文件**:[src/components/onboarding/onboarding-scaffold.tsx](../../src/components/onboarding/onboarding-scaffold.tsx)、
[src/components/mobile/screen.tsx](../../src/components/mobile/screen.tsx)

### Toast 走 burnt(各平台原生机制),门面在 lib/toast.ts

toast 底层是 `burnt`——**用各平台的系统原生机制**,不是自绘的 JS 横条:
- **iOS**：SPIndicator 胶囊(顶部,`from: "top"`),支持 title + message 两行 + `preset`(done/error/none)图标。是**原生模块**。
- **Android**：系统 `ToastAndroid`(底部,`from: "bottom"`)。burnt 的 Android 侧是**纯 JS**(直接包 RN 内置
  `ToastAndroid`,不调原生);**只显示 `title`、丢弃 `message`**,所以门面在 Android 上把 description 折进 title。

门面 [src/lib/toast.ts](../../src/lib/toast.ts)(`toast.success/info/error/loading/promise/dismiss`)封装了平台差异,
调用点只依赖门面。burnt 是命令式的(`Burnt.toast()` 直接弹),**不需要 `<Toaster>` 宿主组件**。

**为什么不是自绘横条**：瞬时提示应贴平台习惯——iOS 是顶部胶囊(SPIndicator),Android 是底部系统 toast。
先前试过 sonner-native 自绘 card,观感在两端都"不像原生",故换 burnt 用系统机制。

**重编要求(重要)**：burnt iOS = 原生模块(SPIndicator),**新增/改动后 iOS 必须 `expo prebuild` + 重编**才生效
(autolinking 自动接,无需手写 config plugin);**Android 无需重编**(纯 ToastAndroid JS,Metro 热更即可)。
burnt 的 iOS `BurntModule` 在 import 时 `requireNativeModule('Burnt')`,未重编的 iOS app 加载会崩——验证 iOS 前务必先重编。

**相关文件**：[src/lib/toast.ts](../../src/lib/toast.ts),[src/app/_layout.tsx](../../src/app/_layout.tsx)(已移除 toast 宿主)
