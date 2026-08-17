---
name: SwarmDrop
description: 可信设备之间的加密数据通道。
colors:
  harbor-teal-light: "oklch(0.583 0.105 177.1)"
  harbor-teal-dark: "oklch(0.641 0.115 177.6)"
  brand-ink-light: "oklch(0.516 0.093 178.2)"
  brand-ink-dark: "oklch(0.828 0.12 179)"
  graphite-ink: "oklch(0.145 0 0)"
  cloud-surface: "oklch(1 0 0)"
  muted-surface: "oklch(0.97 0 0)"
  destructive: "oklch(0.577 0.245 27.325)"
typography:
  headline:
    fontFamily: "system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif"
    fontSize: "15px"
    fontWeight: 600
    lineHeight: "1.2"
  body:
    fontFamily: "system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: "1.4"
  label:
    fontFamily: "system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif"
    fontSize: "12px"
    fontWeight: 500
    lineHeight: "1.35"
  mono:
    fontFamily: "ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace"
    fontSize: "12px"
    fontWeight: 500
    lineHeight: "1.4"
    fontFeature: "tabular-nums"
rounded:
  control-sm: "6px"
  control: "8px"
  control-lg: "14px"
  panel-sm: "18px"
  panel: "24px"
  full: "9999px"
spacing:
  xs: "4px"
  sm: "8px"
  md: "16px"
  lg: "24px"
components:
  button-primary:
    backgroundColor: "{colors.harbor-teal-light}"
    textColor: "{colors.graphite-ink}"
    rounded: "{rounded.control}"
    height: "36px"
    padding: "0 16px"
  input-default:
    backgroundColor: "{colors.cloud-surface}"
    textColor: "{colors.graphite-ink}"
    rounded: "{rounded.control}"
    height: "36px"
    padding: "4px 12px"
  panel:
    backgroundColor: "{colors.cloud-surface}"
    textColor: "{colors.graphite-ink}"
    rounded: "{rounded.panel}"
    padding: "{spacing.md}"
---

# Design System: SwarmDrop

这是跨端设计契约，拥有桌面、移动和 Web 共用的视觉语言与交互语义。平台实现差异分别写在 [src/DESIGN.md](src/DESIGN.md)、[mobile/DESIGN.md](mobile/DESIGN.md) 和 [docs/DESIGN.md](docs/DESIGN.md)；它们不得重新定义本契约。

## Overview

**创意北极星：“加密工作台”**

SwarmDrop 是在可信设备间传递信息的精确、安静工具。它要技术上诚实，而不是消费化卖萌：少量明确的颜色、清晰的结构，以及对连接、信任、进度和送达状态的如实呈现。

它不是通用 SaaS 仪表盘，也不是密集的企业后台。它是聚焦的工作台：一次一个主任务、始终保留设备上下文、不让装饰和状态争抢注意力。

**关键特征：**

- Harbor Teal 是唯一的品牌操作强调色；语义色只传达状态。
- 控件平面且直接；深度只属于结构容器，不能属于可输入或可点击控件。
- 字面机器数据使用等宽字与 tabular numerals。
- 三端共享信息顺序和交互含义，同时服从各自的原生输入模型。

## Colors

Harbor Teal 锚定可信操作；中性色承担绝大多数布局；语义 token 传达系统状态。

### Primary

- **Harbor Teal：**主操作填充、选中控件、发送入口与焦点；按当前主题使用明/暗值。
- **Brand Ink：**青绿色的文字/图标形态；浅色背景上的小文字使用它，不能用较浅的填充色直接写在白底上。

### Neutral

- **Cloud Surface：**主应用、面板和 popover 表面。
- **Graphite Ink：**浅色主题主前景；每端映射为可访问的深色主题等价值。
- **Muted Surface：**次级控件和辅助区域的安静填充。

### Named Rules

**单一强调色规则。** Harbor Teal 是唯一品牌强调色。`success`、`warning`、`destructive` 与 `info` 只能表达各自的语义，不能装饰页面。

**状态墨水规则。** 语义基色用于填充、圆点和图标；浅色或同色 tint 上的文字使用对应 `-ink` token。状态不能只用颜色表达，必须有文字或图标。

## Typography

**正文：**平台系统无衬线字。它让各操作系统上的设备信息都保持熟悉、易读。

**机器字：**平台等宽字，并启用 tabular numerals。

### Hierarchy

- **Headline：**聚焦页面或分区标题；先用字重和留白，再增加字号。
- **Body：**说明、表单输入和操作标签。
- **Label：**元数据、计数、徽标和帮助文本；不得低于平台无障碍下限。
- **Mono：**peer ID、配对码、hash、速度、字节数、时间戳及其他字面值。

### Named Rules

**机器事实规则。** 用户可以复制、校验或比较的值使用带 tabular numerals 的等宽字。人类文案和收到的文本使用正文无衬线字。

## Layout

使用紧凑的四级节奏（4 / 8 / 16 / 24）。屏幕先建立一个焦点任务，再按目标摘要、主要内容、少量操作组织。重复卡片不能代替信息层级。

三端保持相同任务顺序，即使页面拓扑不同。设备是发送上下文：发送必须从已选设备开始，不能成为常驻导航目的地。窄屏优先保留主要状态与操作，再取舍装饰和次要元数据。

文本投递统一顺序为：目标设备 → 文件/文本模式 → 编辑或粘贴 → 字节限制 → 明确发送 → 真实结果。Outbox 服务发送上下文；Inbox 始终只代表收到的内容。

## Elevation & Depth

控件静止时保持平面，以克制边框或阴影区分。结构面板仅在目标端有合适环境层时使用柔和玻璃材质；每个玻璃表面必须有 reduced-transparency 的平面降级。动效必须为 `prefers-reduced-motion` 冻结或减弱。

### Named Rules

**平面控件、玻璃框架规则。** 按钮、输入框、文本编辑器和可选行不使用模糊。玻璃属于装载内容的容器，不属于用户要操作的控件。

## Shapes

形状语言有两档：控件为 6–14px，结构面板为 18–24px。胶囊只用于紧凑状态和筛选，不用于任意标签。所有可触操作的命中区至少为 44 × 44 CSS 像素或平台等价尺寸。

## Components

### Buttons

- **Primary：**Harbor Teal 填充配目标端可访问前景；一个聚焦区域只保留一个主操作。
- **Secondary：**中性、平面，视觉上弱于主操作。
- **Focus 与 disabled：**焦点无需依赖颜色即可看见；当禁用原因重要时，需要说明原因。

### Inputs and Text Editors

- **样式：**中性表面、清晰边框、可见焦点。
- **文本投递：**粘贴和复制都由用户明确触发；焦点、窗口激活、通知和接收都不能触发剪贴板操作。

### Device Cards

- **合同：**显示身份、带圆点和文字的在线状态、已知的连接/信任信息，以及预选该设备的发送入口。
- **离线：**禁用发送和整卡激活；不能隐藏设备或暗示其仍可到达。

### Status and Trust

- **信任：**owned 用 `primary`，collaborator 用 `muted`，temporary 用 `warning`，blocked 用 `destructive`。
- **状态：**除视觉状态外还要有文字结论；错误和机器诊断在作为字面数据时保持可复制、不翻译。

### Content Mode Selector

- **职责：**在目标设备选定后切换既有文件发送和显式文本发送。
- **规则：**它是紧凑分段控件，不是移动端第五个文件来源、常驻导航项或聊天编辑器。
- **位置：**贴在目标设备上下文所在的水平带上，不独占横向带。宽度按内容自适应、不设固定宽。三端各自找自己那条上下文带：桌面在页头（设备摘要条右侧已被「已选内容」计量占用），移动端在设备行右侧（该端计量在底部操作栏，那一格是空的），Web 在设备行「更换」按钮左侧。落点不同、语义相同，与「三端保持相同任务顺序，即使页面拓扑不同」一致。
- **高度按输入方式定，不按端定：**纯指针端 36px（桌面页头，只有鼠标）；**任何可能被手指点到的地方 44px**——移动端如此，**Web 应用区同样如此**（它是移动优先的响应式界面，手机浏览器上这就是触控目标）。触控端的紧凑只能靠缩宽。
- **语义：**tablist / tab，`aria-controls` 与对应面板互指，方向键在组内移动焦点并带走焦点。
- **图标：**文件用 `FileText`、文本用 `Type`，三端同一套；`Clipboard` 说的是「从剪贴板来」，那是面板内粘贴按钮的语义，不是这一档的。
- **与它同排的计量随模式变。**「已选内容 / N 项 · 大小」只属于文件模式；文本模式该位置显示字节用量。同一个数字不在一屏里出现两次。

## Do's and Don'ts

### Do:

- **Do** 在三端保持同名语义 token 和相同状态含义。
- **Do** 明确、如实地呈现连接、信任、待确认和送达状态。
- **Do** 改动某端视觉实现前先阅读该端文档。
- **Do** 本地化所有可见和可访问文字；字面机器值保持原样且可复制。

### Don't:

- **Don't** 发明仅属于某端的品牌色、视觉隐喻或信任颜色映射。
- **Don't** 把发送、传输或 Outbox 变为常驻顶级导航。
- **Don't** 用文本文件或剪贴板副作用替代文本投递。
- **Don't** 为了“看起来有设计”而堆渐变、卡片网格或密集表格。
