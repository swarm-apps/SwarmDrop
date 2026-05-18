# SwarmDrop-RN UI v2 设计说明

> 与桌面端 v2(`SwarmDrop/dev-notes/design/design-2.pen`)对齐的移动端重构方案。
> 蓝本:`SwarmNote-RN` 的技术栈与组件模式(NativeWind v5 + Lingui 6 + Bottom Sheet)。

---

## 1. 设计目标

- **被动接收模型一致** —— 接收始终由全局 `TransferOfferHost` 处理,主屏不出现"发送/接收" Tab。
- **手机原生体感** —— 主屏单 ScrollView,Drawer 装二级入口,Bottom Sheet 装临时操作。
- **设计语言对齐桌面** —— neutral 色板、shadcn 风格 token、统一的 `StatusPill` / `DeviceCard` 语义。
- **降低上手成本** —— 进入即看到"我的设备"和"最近传输",不需要切 Tab 找入口。

---

## 2. 技术栈

直接锚定 `SwarmNote-RN@0.4` 的依赖版本,避免双仓栈飘移。

| 类别 | 依赖 | 版本 |
|------|------|------|
| 样式 | `nativewind` | `5.0.0-preview.3` |
| 样式编译 | `react-native-css` | `^3.0.7` |
| Bottom Sheet | `@gorhom/bottom-sheet` | `^5.2.9` |
| Radix RN 移植 | `@rn-primitives/*` | `^1.4.0`(dialog / dropdown-menu / popover / progress / select / separator / switch / tabs / toggle / slot / portal / label / avatar / checkbox / radio-group / alert-dialog) |
| i18n | `@lingui/core` `@lingui/react` | `^6.0.0` |
| 设备语言 | `expo-localization` | `~55` |
| OTP 输入 | `input-otp-native` | `^0.5.0` |
| 触感 | `expo-haptics` | `~55` |
| 工具 | `class-variance-authority` `clsx` `tailwind-merge` | latest |
| Toast | `react-native-notifier` | `^2` |
| 键盘 | `react-native-keyboard-controller` | `^1.21` |
| Drawer | `@react-navigation/drawer` | `^7` |

i18n 初期 locale 只发 **zh-Hans / en**,后续再补对齐桌面 8 locale。

---

## 3. 目录结构

```
src/
├── app/
│   ├── _layout.tsx                — Root Stack(Drawer 在 (main))
│   ├── index.tsx                  — 路由分发(保留)
│   ├── onboarding/                — 保留现状(welcome / setup)
│   ├── (main)/
│   │   ├── _layout.tsx            — Drawer 包裹
│   │   ├── index.tsx              — 主屏:我的设备 + 最近传输
│   │   └── transfer/
│   │       ├── index.tsx          — 传输历史列表
│   │       └── [sessionId].tsx    — 传输详情
│   ├── settings/                  — modal presentation
│   │   ├── _layout.tsx
│   │   ├── index.tsx              — 入口列表
│   │   ├── general.tsx
│   │   ├── language.tsx
│   │   ├── theme.tsx
│   │   ├── network.tsx
│   │   └── about.tsx
│   ├── pairing/                   — 保留(从 PairingRequestHost 等场景跳转)
│   │   ├── input-code.tsx
│   │   ├── found-device.tsx
│   │   └── success.tsx
│   └── send/
│       └── select-device.tsx
├── components/
│   ├── ui/                        — shadcn-style 移植(button/card/badge/dialog/sheet/progress/switch/separator/text/input/select/tabs/skeleton/tooltip/dropdown-menu/alert-dialog/checkbox/radio-group/avatar/label/popover/toggle/collapsible/icon/textarea/native-only-animated-view)
│   ├── pairing-sheet.tsx          — 新:配对 BottomSheetModal(双 tab)
│   ├── device-card.tsx            — 新:2 列 grid 卡片
│   ├── recent-transfer-row.tsx    — 新:最近传输行
│   ├── status-pill.tsx            — 新:节点状态 pill
│   ├── drawer-content.tsx         — 新:Drawer 内容(状态 + 导航 + 本机信息)
│   ├── transfer-offer-host.tsx    — 保留接口,内部 Dialog 用新 UI 重写
│   ├── pairing-request-host.tsx   — 同上
│   └── update-host.tsx            — 保留
├── i18n/
│   ├── lingui.ts
│   ├── LinguiProvider.tsx
│   └── languageDetector.ts
├── locales/
│   ├── zh-Hans/messages.po
│   └── en/messages.po
├── hooks/
│   └── useThemeColors.ts          — 从 CSS 变量读 JS 颜色
├── lib/
│   ├── utils.ts                   — cn() helper
│   ├── toast.ts                   — react-native-notifier 包装
│   ├── theme-persistence.ts       — 主题持久化
│   └── device-platform.ts         — getDeviceIcon(os)
├── stores/                        — 保留全部
└── global.css                     — 新:NativeWind v5 + CSS 变量(neutral 色板)
```

---

## 4. 导航树

```
Root Stack (expo-router)
├── /index                          [redirect]
├── /onboarding
│   ├── welcome
│   └── setup
├── /(main)                         [Drawer]
│   ├── index                       — 主屏(我的设备 + 最近传输)
│   └── transfer
│       ├── index                   — 传输历史
│       └── [sessionId]             — 传输详情
├── /settings                       [presentation: modal]
│   ├── index
│   ├── general | language | theme | network | about
├── /pairing                        [slide_from_right]
│   ├── input-code
│   ├── found-device
│   └── success
└── /send
    └── select-device
```

Drawer 用 `expo-router` 的 `Drawer` layout 包 `(main)` 子组,主屏 + 传输历史 + 传输详情都在 Drawer 内。Settings 走 modal 不进 Drawer(和桌面"setting 是临时入口"语义一致)。

---

## 5. 主屏布局

```
┌─────────────────────────────┐
│ ☰   SwarmDrop      ●运行中 │ ← header: drawer toggle + status pill
├─────────────────────────────┤
│                             │
│ 我的设备 (3)         + 添加 │ ← section title + 添加设备(打开 PairingSheet)
│                             │
│ ┌──────┐ ┌──────┐           │
│ │ 📱   │ │ 💻   │           │
│ │ MBP  │ │ iPad │           │
│ │ 在线 │ │ 离线 │           │
│ └──────┘ └──────┘           │
│ ┌──────┐                    │
│ │ 🖥   │                    │
│ │ NUC  │                    │
│ │ 在线 │                    │
│ └──────┘                    │
│                             │
│ 最近传输          查看全部 →│ ← 点查看全部 → /transfer
│                             │
│ ─ photo.jpg →MBP    ✓ 完成  │
│ ─ video.mp4→NUC     ↓ 50%  │
│ ─ doc.pdf  →iPad    ⚠ 失败 │
│                             │
└─────────────────────────────┘
```

**交互**

| 元素 | 行为 |
|------|------|
| 顶部 ☰ | 打开 Drawer |
| StatusPill | 显示节点状态(running/starting/stopped/error),点击 → 启停节点 |
| 设备卡片 | 整张 `Pressable`,在线时点击 → 选文件 → push `/send/select-device` 预选该对端 |
| 设备卡片(离线) | 仍可点击查看详情(可选),或显示 disabled 视觉 |
| "+ 添加" 按钮 | 打开 `PairingSheet`(BottomSheetModal) |
| 最近传输行 | 点击 → push `/transfer/[sessionId]` |
| "查看全部" | push `/transfer` |
| 主屏空态(0 设备) | 中央插画 + "通过配对码连接你的第一台设备"(直接放 `[+ 添加设备]` CTA) |

**设备卡片字段**

- 平台图标(根据 osInfo.os 选 lucide-react-native:Monitor/Smartphone/Tablet/Laptop)
- 设备名(hostname,1 行省略)
- 状态徽标(在线绿点 / 离线灰点 / 可接收蓝点)
- 平台 + 操作系统(small caption,e.g. `macOS · darwin`)

---

## 6. Drawer 内容

```
┌──────────────────────┐
│                      │
│  ●  运行中           │ ← StatusPill(大尺寸,Drawer 顶部强调)
│  (点击启动/停止节点) │
│                      │
│  ──────────────────  │
│                      │
│  🏠  主页           │ ← active(高亮)
│  📋  传输历史        │
│  ⚙   设置           │
│  ℹ   关于           │
│                      │
│  (spacer)            │
│                      │
│  ──────────────────  │
│  本机                │
│  MBP-yexiyue         │
│  12D3KooWBxYzAb…    │ ← PeerID 缩略 + 点击复制
│  v0.1.0              │
└──────────────────────┘
```

- 用 `DrawerContentComponentProps` 注入自定义 content。
- 路由跳转用 `props.navigation.navigate(...)`。
- 长按 PeerID 全文复制 + `expo-haptics` 触感反馈。

---

## 7. 配对 Bottom Sheet

```
╔════════════════════════════════╗
║              ─                 ║ ← drag handle
║                                ║
║   ┌──────────────────────────┐ ║
║   │ [生成码] │  输入码          │ ║ ← Tabs(@rn-primitives/tabs)
║   └──────────────────────────┘ ║
║                                ║
║   【生成码 tab】                ║
║   8  4  3  2  7  1            ║ ← 大字号 OTP-style 展示
║                                ║
║   ⏱  还有 01:42 过期           ║
║                                ║
║   [↻ 重新生成]    [复制]       ║
║                                ║
║   ──────────────────────────   ║
║   📡 正在等待对端连接...        ║
║                                ║
╚════════════════════════════════╝
```

切到"输入码" tab:

```
║   【输入码 tab】                ║
║                                ║
║   ┌─┬─┬─┬─┬─┬─┐               ║ ← input-otp-native
║   │ │ │ │ │ │ │               ║
║   └─┴─┴─┴─┴─┴─┘               ║
║                                ║
║   [继续 →]                     ║ ← 进 /pairing/found-device
```

- 使用 `BottomSheetModal` + `enableDynamicSizing`。
- 顶部用 `@rn-primitives/tabs`,和桌面 Dialog 内 tab 切换一致。
- "生成码"展示由 `usePairingCodeGenerator` 驱动(已有),展示倒计时和过期重生成。
- 关闭 sheet 不打断已生成的码(让用户切走再切回还看得到)。

---

## 8. 全局 Host(保留,内部重写 UI)

`app/_layout.tsx` 已挂载:
- `<PairingRequestHost />` —— 收到配对请求 Dialog
- `<TransferOfferHost />` —— 收到文件 Dialog(带文件列表 + 保存位置)
- `<UpdateHost />` —— App 升级提示

UI v2 中 **保留 Host 接口** 和事件订阅逻辑,但**用新的 `components/ui/dialog.tsx`** 重写视图层,使其和桌面 v2 的设计语言一致。

---

## 9. 设计 Token(NativeWind v5 + CSS 变量)

`src/global.css`(抄 SwarmNote-RN 的样板,色值与桌面 shadcn neutral 对齐):

```css
@tailwind base;
@tailwind components;
@tailwind utilities;

@layer base {
  :root {
    --background: 0 0% 100%;
    --foreground: 222.2 84% 4.9%;
    --card: 0 0% 100%;
    --card-foreground: 222.2 84% 4.9%;
    --primary: 217.2 91.2% 59.8%;
    --primary-foreground: 210 40% 98%;
    --secondary: 210 40% 96.1%;
    --secondary-foreground: 222.2 47.4% 11.2%;
    --muted: 210 40% 96.1%;
    --muted-foreground: 215.4 16.3% 46.9%;
    --accent: 210 40% 96.1%;
    --accent-foreground: 222.2 47.4% 11.2%;
    --destructive: 0 84.2% 60.2%;
    --destructive-foreground: 210 40% 98%;
    --success: 142 76% 36%;
    --success-foreground: 210 40% 98%;
    --warning: 38 92% 50%;
    --border: 214.3 31.8% 91.4%;
    --input: 214.3 31.8% 91.4%;
    --ring: 217.2 91.2% 59.8%;
    --radius: 0.625rem;
  }

  .dark {
    --background: 222.2 84% 4.9%;
    --foreground: 210 40% 98%;
    /* ... 对应 dark 色值 */
  }
}
```

- `useThemeColors()` 抄 SwarmNote-RN 实现:`useUnstableNativeVariable` 读出 `hsl(...)` 给图标 `color` prop。
- `useNavTheme()` 喂给 `ThemeProvider`(React Navigation)。
- 主题切换通过 `nativewind` 的 `useColorScheme()` + `theme-persistence.ts`。

**与桌面端的关系**:桌面端用 Tailwind v4 `@theme` 块,RN 端用 v3 风格 `@layer base` 配 NativeWind v5 — 色值表对齐即可,语法差异不影响视觉一致性。

---

## 10. i18n 集成

完全复用 SwarmNote-RN 模板:

```ts
// src/i18n/lingui.ts
const loaders: Record<SupportedLanguage, () => Promise<{ messages: Messages }>> = {
  "zh-Hans": () => import("../locales/zh-Hans/messages.po"),
  en: () => import("../locales/en/messages.po"),
};
```

- `lingui.config.ts` 在仓库根。
- `pnpm i18n:extract` 自动提取 `<Trans>` / `t\`...\`` 到 .po。
- `LinguiProvider` 在 root layout 包裹一切。
- `expo-localization` 探测系统语言;`AsyncStorage` 持久化用户偏好。

**首批翻译范围**(zh-Hans 为源):
- 主屏:`我的设备` / `添加` / `最近传输` / `查看全部` / `运行中` / `启动中` / `已停止` / `错误` / `暂无设备`
- 配对 Sheet:`生成码` / `输入码` / `重新生成` / `复制` / `继续` / `还有 {time} 过期`
- Drawer:`主页` / `传输历史` / `设置` / `关于` / `本机`
- 设置子页:延续桌面端 .po 现有 msgid
- 全局 Host:Dialog 文案对齐桌面

---

## 11. 实施路径

分 5 个 commit,每个 commit 都能独立 build+lint 过:

### Commit 1 — 基建

- 装依赖(见 §2)
- `metro.config.js` 接入 NativeWind v5
- `babel.config.js` 加 `@lingui/babel-plugin-lingui-macro` 和 `nativewind/babel`
- `lingui.config.ts` + `src/i18n/*` + 空 .po 占位
- `src/global.css` 设计 token
- `src/lib/utils.ts` cn()
- `src/hooks/useThemeColors.ts`
- `src/lib/theme-persistence.ts`
- `app/_layout.tsx` 包 `GestureHandlerRootView` / `KeyboardProvider` / `SafeAreaProvider` / `ThemeProvider` / `LinguiProvider` / `BottomSheetModalProvider` / `PortalHost` / `NotifierRoot`
- `app.json` plugins 加 `expo-localization`

**验收**:`pnpm typecheck` + `pnpm lint` 过,启动后空白屏不崩。

### Commit 2 — UI 组件库

- 整套 `components/ui/*` 从 SwarmNote-RN 拷贝(button / card / badge / dialog / sheet 不需要,bottom-sheet 单独用 @gorhom / progress / switch / separator / text / input / select / tabs / skeleton / tooltip / dropdown-menu / alert-dialog / checkbox / radio-group / avatar / label / popover / toggle / collapsible / icon / textarea / native-only-animated-view)
- 创建一个 `/explore`(临时)页验证组件能渲染。

**验收**:`/explore` 能看到所有 UI 原子组件渲染正常。

### Commit 3 — 主屏 + Drawer

- `app/(main)/_layout.tsx` 用 `Drawer`(`@react-navigation/drawer`)包
- `components/drawer-content.tsx`(StatusPill + 4 导航 + 本机信息)
- `app/(main)/index.tsx` 重写:Header / 设备 grid / 最近传输
- `components/device-card.tsx`(2 列 grid 卡片)
- `components/recent-transfer-row.tsx`
- `components/status-pill.tsx`
- 删除旧 `(main)/index.tsx` 中所有 `StyleSheet.create` 样式
- i18n:所有文案过 `<Trans>`/`t``

**验收**:主屏可见、Drawer 可拉出、点击设备进 select-device 不崩、状态 pill 可启停节点。

### Commit 4 — 配对 Sheet + 设置子页

- `components/pairing-sheet.tsx`(双 tab)
- 主屏"+ 添加"按钮 trigger sheet
- `app/(main)/index.tsx` 旧的 "配对" section 移除(已经迁到 sheet)
- `app/settings/*` 6 个子页(general / language / theme / network / about + 入口列表)
- 复用 SwarmNote-RN 的 `setting-row.tsx` 组件

**验收**:配对码生成/过期/复制/重新生成正常,设置项每个都能改并持久化。

### Commit 5 — Host 重写 + 传输页

- `transfer-offer-host.tsx` 用新 Dialog UI 重写(对齐桌面 `TransferOfferDialog`)
- `pairing-request-host.tsx` 同上(对齐桌面 `ConnectionRequestDialog`)
- `app/(main)/transfer/index.tsx`(历史列表,filter + 清空)
- `app/(main)/transfer/[sessionId].tsx`(详情:进度 / 暂停 / 续传 / 打开文件夹)
- 复用桌面 v2 的图标和文案

**验收**:接收文件 dialog 弹出、传输历史可看、详情页可暂停续传。

---

## 12. 与桌面端 v2 的关系

| 桌面元素 | 移动端对应 |
|---------|-----------|
| `AppTopBar`(顶栏 + 面包屑) | RN header(`☰` + 标题 + StatusPill);Drawer + Stack 替代面包屑 |
| 4 列设备 grid | 2 列设备 grid |
| `AddDeviceMenu`(DropdownMenu) | "+ 添加" → PairingSheet |
| 桌面 `/pairing/{generate,input-code}` 独立页 | PairingSheet 双 tab |
| `TransferOfferDialog` | `TransferOfferHost`(已挂全局,UI 重写) |
| `ConnectionRequestDialog` | `PairingRequestHost`(同上) |
| `StartNodeSheet` / `StopNodeSheet` | Drawer 顶部 StatusPill 直接控制(无需弹窗) |
| 设置页(单页) | 设置入口 + 子页(modal presentation) |
| 传输列表 + 详情 | `/transfer` + `/transfer/[sessionId]` |

**保留的桌面已确立原则**(memory 已记):

- 配对是主动行为,接收是被动行为(主屏永远不出现"接收" Tab)。
- 设备卡片整张可点击。
- StatusPill 四态(running/starting/stopped/error)。

---

## 13. 不在本期范围

- 推送(`expo-notifications`)接入接收提醒 —— 留给后续。
- iOS 文件 Share Extension —— 留给后续。
- Android Quick Share Intent —— 留给后续。
- 设备分组 / 标签 —— 留给后续。
- LAN-only / 强制 relay 开关 —— 已在桌面设置中,RN settings/network 子页保留入口。
