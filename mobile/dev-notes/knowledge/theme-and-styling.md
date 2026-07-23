# Theme & Styling

## 概览

NativeWind 5（preview）+ Tailwind v4 + @rn-primitives/* 组成 UI 层；`tailwind-merge` +
`clsx`（合成的 `cn` helper 在 [src/lib/utils.ts](../../src/lib/utils.ts)）做 className 合并；
图标走 `lucide-react-native`；色彩走 CSS 变量（`src/global.css`）+ `useThemeColors` hook 拉对应
JS 值。主题持久化在 [src/lib/theme-persistence.ts](../../src/lib/theme-persistence.ts)。

### 移动端 primary 按钮用深青绿底 + 白字/白图标

Logo 主体色是 Harbor Teal `#0F8F7A`，但它承载白色小字只有约 4.0:1；移动端按钮如果改成黑字又会显得
闷、不够清楚。`src/global.css` 因此把 UI action fill 定为更深一档的 Action Teal `#087968`，
`--primary-foreground` 定为白色。所有 primary 按钮文案用 `text-primary-foreground`，lucide 图标和
spinner 用 `colors.primaryForeground`，不要在按钮里手写黑色或直接把 logo teal 当按钮底。

浅色模式主背景保持白色，重复内容 surface（卡片、设备行、列表行）用更淡一档的 Mist Surface `#F5FAF8`；
app icon / splash 的 Brand Mist `#F0FBF7` 不直接铺满所有卡片，避免整屏泛绿。暗色模式背景从近黑改为
Night Background `#121E20`，卡片 `#18282B`，muted/border/input `#203538`，避免大面积黑底太硬。

**相关文件**：[src/global.css](../../src/global.css)，[DESIGN.md](../../DESIGN.md)

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

非主题里的固定色（amber-500、slate-500）直接写 hex：

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

### 按钮类原语的 `className` 不能漏进 `buttonTextVariants`（否则文案左对齐）

shadcn-rn 的按钮包裹会把子 `<Text>` 的 class 塞进 `TextClassContext`。若把调用方传的
**布局** `className`（如 ConfirmDialog 给双键传的 `flex-1`）**同时**喂给 `buttonTextVariants`，
Text 会被撑成 `flex-1` 铺满按钮宽，RN Text 默认 `textAlign:left` → 文案贴左（看着「没居中」）。
`buttonVariants` 基类已 `items-center justify-center`，Text 只要**不**被撑开就会居中。

**正确做法**：`className` 只进按钮容器（`cn(buttonVariants(...), className)`），
`TextClassContext` 只传 `buttonTextVariants({ variant })`——不带 `className`。规范 `Button`
（button.tsx）本就是这样。

**不要做**：`buttonTextVariants({ variant, className })` —— `alert-dialog.tsx` 曾这样，导致
所有 ConfirmDialog 双键文案左对齐；已修。**`toggle.tsx:54` 目前仍有同款泄漏**（Toggle 暂无
引用，潜伏 bug），后续复用 Toggle 前先照此修。

**相关文件**：[src/components/ui/alert-dialog.tsx](../../src/components/ui/alert-dialog.tsx)，
[src/components/ui/button.tsx](../../src/components/ui/button.tsx)，[src/components/ui/toggle.tsx](../../src/components/ui/toggle.tsx)

### 状态语义色当「文字」必须用 `-ink` 变体（State Ink Rule）

`--success` / `--warning` / `--destructive` / `--primary` 这几个饱和色是给**填充 / 圆点 / 图标**
调过的，当**小字**压在浅底或同色 `/10`~`/15` tint 上时对比度全不达 WCAG AA（amber 只有 ~2:1，
green ~3.3:1，red ~3.5:1，teal ~4.0:1）。所以 `global.css` 另有一套高对比文字变体
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
放 Skeleton 块;按钮内、输入框内的行内 spinner 仍用 `ActivityIndicator`(primary 底按钮内颜色必须
`colors.primaryForeground`,不要 `colors.background` 或手写黑色,按钮前景规则统一由 primary token 承担)。

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

### 二维码白卡是「主题真空区」,内部文字色必须写死

配对二维码一律深模块 + 固定白底(不随暗色反色,摄像头对反色 QR 识别差),所以压在白卡上的
覆盖层文案 / 图标一律写死 slate 深色(`text-slate-700`、`#64748b`),错误态用 `red-600`。
用 `text-muted-foreground` 这类主题 token 会在暗色主题下翻成浅灰、在白底上不可读。

码位的四种失效态(生成中 / 已过期 / 生成失败 / 尚未生成)用**覆盖层**压在码面上
(码本体降到 `opacity 0.14`),不要把 `<InviteQr>` 整个换成一行灰字——那既撞
「空态不要裸一行灰字」,也让卡片高度跳变。动作按钮不放码面,放下方拇指区
(有效时=刷新 / 复制,失效时=整宽「重新生成邀请」),一屏只出现一个主动作。

矩阵渲染按**行游程合并**成 `<Rect width=w>`(而不是每个深模块一个 1×1 Rect):
v11-12 的码约 2600 个格子,合并后 SVG 节点降到 1/3 左右。

**相关文件**:[src/components/pairing/invite-qr.tsx](../../src/components/pairing/invite-qr.tsx)、
[src/components/pairing/invite-exchange.tsx](../../src/components/pairing/invite-exchange.tsx)

### 扫码屏:压暗四周 + 四角标记 + 常驻手电筒,粘贴是同级备用通路

取景不用「一个白色描边方框」,用**四周 scrim(`rgba(0,0,0,0.45)`)挖出中央取景窗 + 四角
L 形标记**:暗场把注意力推到中央,四角在浅色背景(白纸/白屏上的二维码)下也不会糊掉。
提示文案与 scrim 同层放在取景窗正下方,位置随取景窗走,不靠猜垂直间距。

低光是扫码失败的头号原因 → `CameraView` 的 `enableTorch` 接一个**常驻**手电筒按钮
(右上角,开启时反白),不要藏进二级菜单。命中有效邀请时 `Haptics.notificationAsync(Success)`。

扫码与粘贴共用一条 `consumeInvite(raw, invalidHint)` 通路(加锁 → 验签 → 成功 replace /
失败去抖解锁);**相机屏底部按钮是「粘贴邀请」而不是「返回」**——改造前那里是
ClipboardPaste 图标配「返回」文案、行为却是 `router.back()`,图标/文案/行为三者不一致。

**相关文件**:[src/app/pairing/scan.tsx](../../src/app/pairing/scan.tsx)

### 空态两档:全屏 EmptyState / 行内 InlineEmptyState,不要裸一行灰字

`@/components/mobile/screen` 提供两档空态,同一空态语言(dashed 边框 + muted 圆 chip):
- `EmptyState`:整屏/列表级空态,带可选动作按钮(收件箱空态等)
- `InlineEmptyState`:卡片区块/sheet 分组内的行内空态,icon+主文案+副文案,无按钮;
  `pulse` 属性给图标加呼吸脉冲,用于「扫描中/等待中」语义(如附近设备——把"空"表达成
  "正在进行",别用消极的"暂无")

**不要做**:区块空态别再写裸 `<Text>` 一行灰字或私有小灰盒(旧 InlineEmptyText 已删)。

**相关文件**:[src/components/mobile/screen.tsx](../../src/components/mobile/screen.tsx)

### 详情卡里的「左灰标签 + 右对齐值」行用共享 KeyValueRow

传输详情/关于页曾各自手写过同构的行组件(DetailRow/SecurityRow),已收敛为
`@/components/key-value-row` 的 `KeyValueRow`(props: `label`/`value`/`mono?`/`numberOfLines?`,
默认值截 3 行)。新详情屏直接用它,不要再繁殖私有拷贝。已知例外:device/[peerId].tsx 的
`InfoRow` 是历史存量(单行截断 + 11px mono + 居中对齐),差异真实,暂未迁移。

**相关文件**:[src/components/key-value-row.tsx](../../src/components/key-value-row.tsx)

### bottom sheet 串接(收起 A 再弹 B)用 onDismiss 回调,不要 setTimeout 猜动画时长

BottomSheetModal 收起是异步动画,`dismiss()` 后立刻 `present()` 下一个会两 modal 叠加。
正确做法:记一个 pending ref,在 `AppBottomSheet` 的 `onDismiss`(转发自 BottomSheetModal,
动画完全结束后触发)里检查并 present 下一个。不要 `setTimeout(..., 250)` 硬编码动画时长——
换库版本/动画配置就悄悄失效。

**相关文件**:[src/app/(main)/index.tsx](../../src/app/(main)/index.tsx)(AddDeviceSheet 的
openInputCodeSheet/handleSheetDismiss)

### sheet 内非 BottomSheetTextInput 的输入框,键盘避让不会自己生效

@gorhom/bottom-sheet v5 的键盘避让被 `keyboardState.target` 门控:keyboardWillShow 到达时
若 target 未设置,事件只被**缓存**,sheet 纹丝不动被键盘盖住(实测:输入配对码 sheet 里
input-otp-native 的隐形 TextInput 弹键盘,sheet 全程被压在键盘下)。target 只由
`BottomSheetTextInput` 的 onFocus 设置——第三方输入组件(如 input-otp-native)内部是普通
TextInput,永远设不上。

**正确做法**(第三方输入组件无法换成 BottomSheetTextInput 时):
- 组件内 `useBottomSheetInternal()` 拿 `animatedKeyboardState`,在编程式 focus 落地后
  `requestAnimationFrame` 里用 `findNodeHandle(RNTextInput.State.currentlyFocusedInput() as never)`
  取到 node,`animatedKeyboardState.set(s => ({...s, target: node}))` 手动登记;
  卸载时把 target 置回 undefined。缓存的键盘事件会在 target 设置后自动重放,时序安全
- sheet 上配 `keyboardBehavior="interactive"` + `keyboardBlurBehavior="restore"`

**不要做**:
- 不要直接给 input-otp-native 的 OTPInput 传 `onFocus`/`onBlur`——它们写在内部
  `{...props}` 展开**之前**,会被覆盖,库自己的 isFocused/active 状态直接失效

**相关文件**:[src/app/(main)/index.tsx](../../src/app/(main)/index.tsx)(PairingCodeInput)

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

### tab 屏里的常驻底部控件(footer/dock)要靠 SafeAreaView 的 bottom edge 避开 tab bar

iOS 26 的 NativeTabs 是浮动 liquid glass 胶囊,悬浮在 RN 内容之上——贴屏幕底渲染的
footer 会被整条盖住(实测:主屏 dock 按钮被压在 tab bar 毛玻璃下)。修复:带 footer 的
`AppScreen` 在 **iOS** 给 SafeAreaView 的 `edges` 加 `"bottom"`。原生 SafeAreaView 按视图
自身 `safeAreaInsets` 就地测量,在 UITabBarController 子控制器内 bottom **包含 tab bar 高度**。

**Android 不要加 bottom edge**:Android 的 tab bar 是实体占位(内容 view 被压缩,不重叠),
手势导航条又在 tab bar 之下;但 safe-area-context 的 Android 实现直接上报 window 的
navigationBars inset、**不按视图相交计算**——实测给 tab 内内容区多报了整条手势条高度
(63px@Pixel 7),加上后 footer 离 tab bar 凭空多一截空白。

**不要做**:
- 不要用 `useSafeAreaInsets()` 拿这个值——那是 root SafeAreaProvider 的 insets,在 tab
  容器外测量,iOS 上**不含** tab bar 高度
- 不要 hardcode 49/64pt 这类 tab bar 高度魔数
- 无 footer 的屏保持 `edges={["top"]}`,让滚动内容照常延伸到屏幕底

**相关文件**：`src/components/mobile/screen.tsx`(AppScreen 的
`edges={footer && Platform.OS === "ios" ? ["top", "bottom"] : ["top"]}`)

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

### 详情页三原则:状态横幅 / 预览框只给真内容 / 协议细节收纳(2026-07 详情页精致化)

传输详情与收件箱详情曾被评「廉价感重」,根因与修法沉淀为三条可复用原则:

**1. 终态用左对齐状态横幅,不用居中庆祝块。**
完成/失败/等待态渲染 `StatusBanner`(Surface + `size-11 rounded-full` 状态色 `/15` chip + 15px semibold
标题 + 12px muted 副行),与全页左对齐网格统一;状态色只花在 chip 上,文字保持中性(头部 StatusBadge
已承载彩色状态文字)。副行只放**不与页面其他区块重复的新信息**(如「用时 + 平均速度」),不要复读文件数/
大小。进行中/暂停态保留大 % Display 数字块(那是 DESIGN.md 钦定的唯一 Display 用法)。

**2. 大预览框只给「真的有内容可看」的形态。**
收件箱详情只有图片(真预览)、视频(内联播放器)和文本/剪贴板(真正文摘录)配大预览卡;通用文件/
多文件/缺失态改为标题区行首 `size-14 rounded-xl bg-muted` 类型图标 chip——大而空的占位框
(小图标+filler 描述句)是廉价感的最大来源。多文件的文件名摘录卡也删了:下方「包含内容」列表
已是全量信息,摘录卡纯重复。(2026-07 inbox-file-preview 补充)预览卡的交互:图片点击进应用内
全屏查看器(react-native-image-viewing,捏合缩放);视频 expo-video 原生控制条、不自动播放;
判定都是 file:// only(SAF content:// 走「打开」交系统)。「打开」主动作 = 系统预览
(iOS QuickLook / Android ACTION_VIEW),「分享」是更多操作 sheet 里的显式次级动作——
主按钮弹分享面板是语义错位。

**3. 协议细节不上一级界面(产品原则的 UI 落地)。**
- 文件行 checksum:直接删除(极客需求后续再议);
- 保存位置:传输详情只显示 `…/最后一段目录名`(iOS 沙盒/SAF URI 中间段全是 UUID 噪音),全文收进
  收件箱详情「详情」面板的**展开态**;
- policyNote:reason 存在时只显示 reason,不再拼 `label: reason`(会得到「设备策略自动接收: 可信设备
  策略自动接收」式口吃);
- 同一事实一屏只出现一次:改造前「1 文件 · 2.4 MB」一屏出现 3 次、「3 项」出现 4 次。

**不要做**:
- 终态别配进度条/静态 `Loader2` 图标(不会转,像坏掉);等待态用 `ActivityIndicator`。
- 详情页嵌共享组件时注意语境泄漏:FileTree `mode="select"` 的头是「已选文件」(发送流程用语),
  详情页传 `showHeader={false}` 由页面自己出「文件 | N 项」小节标题。

**相关文件**:[src/app/transfer/[sessionId].tsx](../../src/app/transfer/[sessionId].tsx),
[src/app/inbox/[itemId].tsx](../../src/app/inbox/[itemId].tsx),
[src/core/transfer-types.ts](../../src/core/transfer-types.ts)(projectionPolicyNote)
