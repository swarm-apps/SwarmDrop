# Theme & Styling

## 概览

桌面端 UI 层的项目特有约束：shadcn/ui new-york 风格、Tailwind v4 token、macOS 自定义标题栏、Aurora 背景。本主题只记录"看代码看不出来 / 容易踩坑"的部分，常规 utility 用法直接查 `/tailwind-css-patterns` 或 `/tailwind-design-system`。

## 主题色：青绿主色双 token 体系

### 青绿 primary 填充和文字仍然分 token（--brand 存在的原因）

2026-07 起 primary 是 Harbor Teal（light `oklch(0.583 0.105 177.1)` / dark `oklch(0.641 0.115 177.6)`，源自新 logo 的青绿主体）。logo 中心的赤铜 `#C56A42` 只作为小面积品牌温度，不作为通用 UI action/state 色。

**正确做法**：
- **填充场景**（按钮底、badge 底、开关轨道）：用 `bg-primary` + `text-primary-foreground`（深墨 `#020817`，对青绿底约 5:1）。**不是白字**——白字在青绿底上只有约 4.0:1，小字不够稳
- **文字/图标场景**（链接、accent 图标、彩色标签）：用 `text-brand`（light 深青绿 `#087968` 白底 5.3:1；dark 自动切亮青绿 `#5EE0C8`）。`text-primary` 是填充色，不要直接当小字正文色
- 半透明 wash 用 `bg-primary/10`、`ring-primary/15` 这类 opacity modifier，light/dark 各自微调档位（dark 通常高一档）

**不要做**：
- `text-primary` 当文字色（亮色模式必挂对比度）
- 青绿填充配 `text-white`
- 把赤铜中心色 `#C56A42` 升级成按钮/状态主色
- 新代码再写死 `text-blue-600` / `bg-blue-500/10` 之类蓝色 utility——蓝色 accent 已于主题迁移时全量清除

**相关文件**：`src/index.css`（`--brand` 定义 + `@theme inline` 注册）、`DESIGN.md`（Brand Fidelity Rule）

### destructive 填充必须配高对比前景色

`--destructive-foreground` 用于红色危险操作按钮和 Windows 自绘关闭按钮的悬停图标；亮色与暗色主题都应使用接近白色的前景色。若误配为 `--destructive` 本身，图标和文字会在红色背景上失去对比度。

**正确做法**：
- 保持 `--destructive-foreground` 为浅色，并让 `bg-destructive text-destructive-foreground` 成对出现

**不要做**：
- 把 `--destructive-foreground` 设成与 `--destructive` 相同的红色

**相关文件**：`src/index.css`、`src/components/layout/app-topbar.tsx`

### 连接类型徽章三色保持语义可辨

设备卡的连接类型徽章是语义状态色，不跟品牌色走：局域网=green、打洞=sky、中继=amber。品牌主色现在也是青绿色，任何新的 success/online 语义用法都要先确认与 `text-brand` / `bg-primary` 拉得开距离；赤铜只留给 logo/品牌小点缀，不参与状态编码。

**相关文件**：`src/routes/_app/devices/-components/device-card.tsx`（`connectionConfig`）

## 窗口装饰 (macOS)

### macOS 标题栏走 Overlay 模式

`src-tauri/tauri.conf.json` 里 `titleBarStyle: "Overlay"` + `hiddenTitle: true` + `trafficLightPosition: { x: 12, y: 22 }`。这意味着：

- 顶栏组件需要给左侧留约 70-80px 给红绿灯占位（仅 macOS）
- 自定义顶栏可以横跨整个窗口宽度，不要尝试绘制原生标题
- Windows 端不走 Overlay，顶栏布局要兼容两种平台（用 `cfg!()` 等价的前端判断 + Tauri OS plugin）
- 任何隐藏 `AppTopBar` 的布局（如 onboarding）也必须自行提供顶栏：macOS 留原生红绿灯和拖拽区，Windows/Linux 用一个右侧 `flex` 容器包裹并复用 `WindowControls`（它返回多个同级元素），否则 `justify-between` 会把每个控制按钮拉散；缺少这层 chrome 时，`set_decorations(false)` 后窗口也无法拖动或关闭

**相关文件**：`src-tauri/tauri.conf.json`、`src/components/layout/*`

### 发送流保留全局 AppTopBar

发送、快捷发送与发送进度属于已认证应用内的任务流，而不是独立窗口。它们应显示 `AppTopBar`，让用户保有品牌、节点状态、全局导航和 Windows 窗口控制；仅配对流程继续使用独立全屏 chrome。发送页内部的 `TaskToolbar` 只提供当前任务的返回和上下文。

**正确做法**：
- 在 `_app` layout 仅将 `/pairing` 判为全屏路由
- 在 `AppTopBar` 为 `/send` 提供「主页 > 发送文件」面包屑

**相关文件**：`src/routes/_app.tsx`、`src/components/layout/app-topbar.tsx`、`src/components/layout/task-surface.tsx`

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

ja / ko / es / fr / de 是规划目标，**当前实际只有 zh / zh-TW / en**（见 `lingui.config.ts` 与 `src/locales/`）。加新语言前先确认 Aurora / 字体 fallback 等下游是否准备好。

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

### 弹窗 seeding effect 别依赖整个 store 对象（否则弹窗内改 store 会冲掉编辑态）

弹窗打开时常用 `useEffect` 把 store 持久值 seed 进本地编辑 state（别名、pill 勾选）。**若该 effect 依赖整个 store 派生对象（如 `organization`），而弹窗内又有会 mutate 这个 store 的操作（如"新建分组"）**，mutation 换掉对象引用 → effect 重跑 → 把用户尚未保存的本地编辑连同刚建的东西一起冲掉（xhigh code-review 实锤：别名、pill、新分组自动选中三样全丢）。

**正确做法**：
- effect 只依赖"真正的重置时机"（`[device, open]`），store 对象用 `useRef` 读快照：`const orgRef = useRef(organization); orgRef.current = organization;` 然后 effect 内 `orgRef.current`。既拿最新值又不进依赖，弹窗内 mutation 不再触发重置。
- 派生给 UI 的实时列表（pill 列表）照常 `useMemo(store.groups)`——它**该**跟随 store 实时更新；不能被重置的只是本地"编辑中"state。

**不要做**：
- 把整个 `organization` / store 对象塞进 seeding effect 的依赖数组。

**相关文件**：`src/routes/_app/devices/-components/device-organization-dialogs.tsx`（`DeviceOrganizationDialog` 的 seeding effect + `organizationRef`）

## 二维码不随暗色主题反色

配对邀请二维码本体**一律深模块（`#0a0a0a`）+ 白底**，无论 app 是否暗色主题，套一张白色
圆角卡（摄像头扫码器对「浅前景/深背景」的反色 QR 识别率差）。三端 QR 由 core
`swarmdrop-invite::qr` 统一生成（桌面/web `InviteQr` 组件消费 SVG、RN 消费矩阵用
react-native-svg 画 `<Rect>`），配色固化在渲染端，不接主题 token。

**相关文件**：`src/components/pairing/invite-qr.tsx`、`mobile/src/components/pairing/invite-qr.tsx`

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
- SoftAurora 不要再额外压整体 opacity 或用低 brightness 模拟氛围雾；保持接近 React Bits 默认的 `brightness: 1`，用颜色和玻璃层透明度控制强弱
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

## master-detail 页面（收件箱 / 传输活动）

### 统一走 MasterDetailShell，断点、抽屉方向、自动选首项全局一个标准

收件箱与传输活动共用 `src/components/layout/master-detail-shell.tsx`，不要各页手写响应式/抽屉：
- **宽屏（≥920px）**：`minmax(300px, listMaxWidth)` 列表 + `minmax(0,1fr)` 详情双栏。
- **窄屏（<920px）**：**详情占满 + 列表从左侧抽屉滑出**（遮罩限定内容区、不盖顶栏，面板 `inert` + Esc + 焦点移入）。两页都从左开，方向统一。

**断点单一来源**：`MASTER_DETAIL_QUERY = "(min-width: 920px)"`（`src/hooks/use-media-query.ts`，配 `useIsWideLayout()`）。920 对齐首页设备页的 `min-[920px]` 主分栏——首页收栏时两页同步进抽屉。新 master-detail 页面复用这个常量，不要各写各的断点（曾经 inbox/transfer 各写 1024px，且抽屉一左一右不统一，已收敛）。

**shell 用法**：`list`/`detail` 是 render prop。`list({ closeDrawer })`（选中后关抽屉）；`detail({ openList, isCompact })`——`openList` 窄屏非 null（用 `<OpenListButton>` 唤出列表）、宽屏 null；`isCompact` 决定详情内部滚动 vs 随页面滚动（对应旧 `contained`）。详情组件自包裹 `glass-panel`。

**render prop 里的回调别现场包，会打穿列表行 memo**：`list({ closeDrawer })` 的 `closeDrawer` 是 shell 内 `useCallback` 稳定引用。若在 render prop 里把它和 `selectSession` 现场拼成 `(id) => { selectSession(id); closeDrawer(); }` 再透传给 **memo 化的行组件**（transfer 的 `SessionRow`），这个闭包每帧换引用 → 行 memo 全失效、任一交互（选中/筛选/进度迁移）全量重渲染整列。正确做法：把稳定的 `onSelect` + `onAfterSelect` **分别**传进列表组件，由列表组件内部 `useCallback` 合成后再下发。同理，`SlideDrawer` 的 `onClose` 各调用点都是内联箭头，其 keydown/focus effect 必须用 ref 读最新值、**只依赖 `[open]`**，否则抽屉打开期间父级每渲染一次就解绑重绑监听 + 强制 focus（收件箱窄屏搜索框在抽屉内，等于每敲一字空转一轮）。inbox 的行组件未 memo 化，故只有 transfer 会踩前半条，但两条都以 render prop 传闭包为根因。

**自动选首项**：两页有内容时自动选中首项（详情区默认有内容），仅零条目才显示空态。规则：无有效选中（未选 / 选中项已删除或不在可见列表）且有内容 → 选首个可见项；**不在筛选/搜索切换时强制重选**已有的有效选中，避免选中态跳动。transfer 用 `shown = 选中 ?? items[0]` 派生，避免自动选中 URL 更新前的空窗闪烁。

**选中态放 URL search param**（如 `/transfer?session=xxx`），旧的 `$id` 详情路由用 `beforeLoad` + `redirect` 兜住深链；列表内点击用 `replace: true` 避免堆历史。

**带 chrome 的页面用 `<SlideDrawer>` 而非整个 shell**：发送流（`/send/share-target`）有 TaskToolbar + CommandDock，不是纯 master-detail 页，不能直接套 MasterDetailShell。改用从 shell 导出的 `<SlideDrawer open onClose label>`（左滑、遮罩、Esc、inert、焦点，与 shell 内部同一实现），配 `useIsWideLayout()` 自己组双栏/单栏。**抽屉的定位上下文**：SlideDrawer 用 `absolute inset-0`，外层必须有一个 `relative` 且**非 overflow-auto** 的祖先（否则被裁剪/随滚动漂移），并让它作为抽屉的兄弟节点、与滚动内容平级。

**发送流两页的语义**：`/send`（点设备卡进来，设备已定）主任务是选文件 → 设备收成顶部 mini 摘要条、文件选择占满单栏带滚动；`/send/share-target`（外部打开进来，文件已定）主任务是选设备 → 选设备占主屏、「待发文件」进左抽屉。发送流断点也用 920（同一 `useIsWideLayout`）。

**FileBrowser 的滚动高度链必须完整**：`FileBrowser` 外层本身是 `flex min-h-0 flex-1 flex-col`，其调用方若需要它填满面板剩余空间，也必须是 `flex min-h-0 flex-1 flex-col`。漏掉 `flex-col` 时虚拟列表会按内容撑高，滚动条失效并挤压底部操作栏。发送页、快捷发送左栏/抽屉、传输详情和接收 offer 弹窗都遵循这一约束。

**shadcn Dialog 内要限高滚动**：`DialogContent` 基础是 `grid gap-4`，加 `flex max-h-[85vh] flex-col`（tailwind-merge 会让 flex 覆盖 grid），header/footer `shrink-0`，中间滚动区必须是 **flex-col 容器**（不能只给 `flex-1`），否则内部 FileBrowser 的 `flex-1` 拿不到 flex 父级、按自然高撑破；DialogContent 本身 `overflow:visible` 不裁剪，靠内部滚动收口。

**相关文件**：`src/components/layout/master-detail-shell.tsx`（含 `SlideDrawer`）、`src/routes/_app/inbox/index.lazy.tsx`、`src/routes/_app/transfer/index.lazy.tsx`、`src/routes/_app/send/index.lazy.tsx`、`src/routes/_app/send/share-target.lazy.tsx`、`src/components/transfer/transfer-offer-dialog.tsx`、`src/hooks/use-media-query.ts`

## 设置页（settings）布局与基元

### 设置页统一走「Section → Card → Row」基元 + bento 卡片网格

设置页所有分区一律用 `src/routes/_app/settings/-settings-primitives.tsx` 的三件套，不要每个 section 各自手绘卡片：

- `SettingsSection`：分组标题（品牌色图标 `text-brand` + 标题，可选右侧 `aside`）
- `SettingsCard`：单层 `glass-card`，`overflow-hidden rounded-[20px]`，**不在内部再套 glass-card**
- `SettingsRow`：行内 `border-b border-border/60 last:border-b-0` 分隔，靠分隔线而非「每行独立浮块」

**正确做法**：
- 布局是 bento 不规则卡片网格（`index.lazy.tsx` 内直接用 Tailwind grid utility：`grid-cols-1 md:grid-cols-2 lg:grid-cols-6` + `items-stretch`，各卡用 `col-span-*` 定宽）。行顺序：row1 满宽英雄（设备信息，宽屏身份左 / 指标右）→ row2 关于产品介绍（满宽）→ row3 外观｜通用+传输竖叠｜网络（各 `col-span-2`，三栏等高）→ row4 引导节点(3)｜MCP(3)
- 同一行卡片内容高度不齐会留透明空洞 → grid 用 `items-stretch`，并给该行的 `SettingsSection` / `SettingsCard` 传 `fill`（section `h-full` + card `flex-1` flex-col），矮卡底部由玻璃底色延伸而非透明空洞；竖叠列（通用+传输）让其中一张卡（传输）`flex-1` 撑满列剩余高度。满宽单卡行（设备信息英雄 / 关于）不传 `fill`
- DOM 顺序＝阅读顺序（设备→偏好→网络→引导→MCP→关于），不会像 `column-count` 瀑布流那样跳列
- 不要用 `column-count` 瀑布流，也不要回退成呆板的等宽双列；"不规则"靠 `col-span` 变化 + 自然高度实现
- 外观的主题选择用可视化迷你预览卡（`ThemeOption` / `ThemePreview`：系统＝左右半屏、浅/深＝迷你窗口缩略图 + 选中蓝边），不要用朴素 Select；让内容少的卡在 bento 里也够"充实"
- 「关于」做成产品介绍卡（slogan + 一句话定位 + 核心特性标签 `FeatureTag` + 官网 / GitHub / 更新日志 / 检查更新按钮，文案对齐 README），置于 row2 设备信息下方作为产品展示；更新检查 / 下载 banner 仍挂在此卡底部
- 圆角 scale 收敛：卡片 `rounded-[20px]`、内部小块/控件 `rounded-xl`、胶囊 `rounded-full`
- 强调色统一 `text-brand` / `bg-primary/10`（2026-07 由 `text-blue-600 dark:text-blue-400` 全量迁移而来），不要写死任何 blue/amber 色阶 utility
- 节点设置「改了要重启」的逻辑用 `useNodeRestart()` hook + `NodeRestartBanner`，网络 / 引导节点共用，不要各抄一份

**不要做**：
- 设置页用 CSS 多列瀑布流；不要给 `.settings-workbench .glass-card > div` 这类规则把每行做成独立浮块
- 同一类设置项一会儿规整 Row、一会儿大色块图标卡，视觉权重不统一

**相关文件**：`src/routes/_app/settings/-settings-primitives.tsx`、`src/routes/_app/settings/index.lazy.tsx`（bento grid + `ThemeOption` / `ThemePreview`）、`src/routes/_app/settings/-device-info-section.tsx`（英雄横卡 `lg:flex-row`）、`src/hooks/use-node-restart.ts`

## 列表拖拽排序

### 拖拽排序统一用 @dnd-kit（键盘可达 + reduced motion 降级）

需要用户手动排序的列表（如设备分组管理）用 `@dnd-kit`（`@dnd-kit/core` + `/sortable` + `/utilities`，2026-07 引入），不要再写 `ChevronUp` / `ChevronDown` 逐格挪动的按钮。

**正确做法**：
- 结构：`DndContext`（`sensors` + `collisionDetection={closestCenter}` + `onDragEnd`）→ `SortableContext`（`strategy={verticalListSortingStrategy}`，`items` 传稳定 id 数组）→ 每行 `useSortable({ id })`
- sensors **必须同时配** `PointerSensor`（`activationConstraint: { distance: 6 }` 防误触，别让点击手柄=开始拖）和 `KeyboardSensor`（`coordinateGetter: sortableKeyboardCoordinates`）——键盘可拿起/移动是项目 WCAG AA 要求，纯鼠标拖拽会让可达性相对旧的上下箭头**倒退**
- 拖拽手柄：lucide `GripVertical` + `touch-none cursor-grab active:cursor-grabbing`，`{...attributes} {...listeners}` 只挂在手柄上（不是整行），行内还有 Input（重命名）等可交互元素时尤其重要
- 持久化：`onDragEnd` 用 `arrayMove(ids, from, to)` 算出新顺序，直接把有序 id 数组丢给 store 的 reorder action（如 `reorderDeviceGroups`）；store 内部按数组下标重编号 `sortOrder`。**乐观 + 立即持久化，不需要本地 order state**——zustand 同步更新后列表顺序即生效
- reduced motion：`useMediaQuery("(prefers-reduced-motion: reduce)")` 为真时把 `useSortable` 返回的 `transition` 置 `undefined`（只保留 `transform` 跟手），满足"始终提供降级路径"

**不要做**：
- 只配 `PointerSensor` 不配 `KeyboardSensor`——丢键盘可达性
- 把 `listeners` 挂到整行——行内 Input / 按钮会抢不到指针事件
- 维护一份本地排序 state 再和 store 双写——直接持久化后由 store 派生即可

**相关文件**：`src/routes/_app/devices/-components/device-organization-dialogs.tsx`（`DeviceGroupsDialog` / `SortableGroupRow`）、`src/stores/preferences-store.ts`（`reorderDeviceGroups`）

### 弹窗内拖拽用 DragOverlay + 内联 restrictToVerticalAxis，行样式走单层分组列表

分组管理在 shadcn `Dialog`（`max-h-[85vh]` 内部滚动）里拖拽排序，两点项目特定处理让它从"能用"到"主流好看"：

**正确做法**：
- **被拖项用 `DragOverlay` 渲染**：`DragOverlay` 以 `position:fixed` 在 portal 层绘制，脱离弹窗 `overflow-y-auto` 滚动容器不被裁切，呈现"被拎起"的抬升态；原行用 `isDragging && "opacity-40"` 留成占位空槽。悬浮预览是**纯展示**组件（grip + 名称文本 + 计数 + 删除图标，`bg-popover` + `border-primary/25` + 玻璃投影），不含真实 Input/Button——避免重复的 aria-label 干扰测试。`onDragStart` 记 `activeId`，`onDragEnd`/`onDragCancel` 清空。
- **纵向锁轴不引 `@dnd-kit/modifiers`**：dnd-kit 的 modifier 本质是 `({ transform }) => Transform` 函数，`restrictToVerticalAxis` 内联成 `({ transform }) => ({ ...transform, x: 0 })` 挂到 `DndContext` 的 `modifiers`，避免为一个函数引整包依赖。
- **`dropAnimation` 跟随 reduced motion**：reduced motion 下传 `null` 关掉落位动画，否则用默认（`undefined`）。
- **行样式是单层分组列表，不是每行带边框卡片**：外层一个 `rounded-[16px] border border-border/60 bg-muted/20` 容器，行内 `rounded-[10px] hover:bg-accent/60` + `gap-0.5`，靠容器边界 + 间距分组，避开本文件「玻璃态层级不要做成卡片套卡片」里同源的"边框堆叠暗色发灰"。弹窗内一律用语义 token（`bg-muted`/`bg-popover`/`bg-accent`/`text-brand`），不用 glass 工具类。

**不要做**：
- 在弹窗滚动容器里只靠 `useSortable` 的 in-flow transform 拖拽——被拖项会被 `overflow` 裁切，且缺少抬升质感。
- 为了锁纵轴而引 `@dnd-kit/modifiers` 整包。

**承载方式决策**：分组管理是"短、可逆、偶尔"的任务，且用武之地就是设备页顶部筛选 chip，用**居中 Dialog**（关闭即见筛选条更新），不开独立路由——独立页面会撞"克制的层级"和"厚重企业级后台"反面参照。

**两个分组弹窗共用同一"分组区"骨架**：管理分组弹窗（`DeviceGroupsDialog`，可排序列表）和单设备别名弹窗（`DeviceOrganizationDialog`，多选归属 pill）都把分组内容放进**同款 recessed 容器**（`rounded-[16px] border border-border/60 bg-muted/20 dark:bg-white/[0.02]`）+ 下方一条"创建行"（`Input` + `variant="outline"` 的 `＋新建` 按钮，`disabled={!newGroup.trim()}` 门控）。pill 未选态用 `bg-background` + hairline，选中态 `bg-primary/10 text-brand border-primary/30` + `Check`——两弹窗视觉权重同源。新增同类"分组/标签选择"区沿用这套骨架，不要各画各的。

**相关文件**：`src/routes/_app/devices/-components/device-organization-dialogs.tsx`（`DeviceGroupsDialog` / `SortableGroupRow` / `GroupRowPreview` / `restrictToVerticalAxis` / `DeviceOrganizationDialog`）

## 剪贴板：桌面端复制统一走 Tauri 插件，不用 navigator.clipboard.writeText

桌面 Tauri WebView 里调 `navigator.clipboard.writeText` 会弹**浏览器权限申请**（"允许访问剪贴板？"），在原生 app 里体验很怪异。

**正确做法**：所有复制走 `copyText()`（`src/lib/clipboard.ts`，封装 `@tauri-apps/plugin-clipboard-manager` 的 `writeText`）——原生系统剪贴板、零提示。

**三处装配缺一不可**：
- `src-tauri/Cargo.toml`：`tauri-plugin-clipboard-manager = "2"`
- `src-tauri/src/setup.rs` 的 `register_plugins`：`.plugin(tauri_plugin_clipboard_manager::init())`
- `src-tauri/capabilities/default.json`：`"clipboard-manager:allow-write-text"`（**只加了插件不加 capability，运行时 invoke 会被拒**）
- 前端：`package.json` 的 `@tauri-apps/plugin-clipboard-manager` + `pnpm install`

**读剪贴板例外**：`src/hooks/use-clipboard-invite.ts` 仍用 `navigator.clipboard.readText()` 做窗口 focus 时的静默轮询感知邀请串——桌面 WebView 读剪贴板无系统提示，故意不切 Tauri 插件（切了要额外申请 read-text 权限）。只有**写**才必须走插件。

**相关文件**：`src/lib/clipboard.ts`、用到的 `-device-info-section.tsx` / `-mcp-section.tsx` / `pairing/generate.lazy.tsx` / `inbox/index.lazy.tsx` / `network/lan-helper-address.tsx`

## LAN 协助地址展示：数据源是 networkStatus.lanHelperAdvertisedAddrs，需自己拼 /p2p/<peerId>

「浏览器快速连接本机」的 ws 地址来自 `networkStatus.lanHelperAdvertisedAddrs`（后端 `crates/core/src/network/manager.rs` 仅在 `provide_lan_helper` 开启时填充为私网监听地址）。这些是**裸监听地址、不含 `/p2p/` 段**，`reserve()` / `connect()` 都要求带 `/p2p/<id>`，故前端 `useLanHelperAddresses` 要 `.filter(a => a.includes("/ws")).map(a => \`${a}/p2p/${peerId}\`)`。

**Zustand 派生别踩坑**：selector 只取稳定的 `s.networkStatus` 引用，`filter/map` 放 `useMemo` 里——直接在 selector 里派生数组每次返回新引用会无限 re-render（见本文件「Zustand selector 与派生数组」）。

**相关文件**：`src/components/network/lan-helper-address.tsx`（`LanHelperAddress` + `useLanHelperAddresses`），装配在 `stop-node-sheet.tsx`（首页状态弹窗）和 `settings/-network-settings-section.tsx`
