---
name: SwarmDrop Mobile
description: 共享 SwarmDrop 设计契约的原生移动适配。
colors:
  harbor-teal: "hsl(170.1 79.7% 31.2%)"
  brand-ink: "hsl(171 87.3% 25.3%)"
  porch-white: "hsl(0 0% 100%)"
  doorway-ink: "hsl(222.2 84% 4.9%)"
  mist-surface: "hsl(156 33.3% 97.1%)"
  threshold-line: "hsl(161.5 23.6% 89.2%)"
typography:
  display:
    fontFamily: "System"
    fontSize: "30px"
    fontWeight: 700
    lineHeight: "36px"
  title:
    fontFamily: "System"
    fontSize: "15px"
    fontWeight: 600
    lineHeight: "20px"
  body:
    fontFamily: "System"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: "20px"
  label:
    fontFamily: "System"
    fontSize: "12px"
    fontWeight: 500
    lineHeight: "16px"
rounded:
  control-sm: "6px"
  control: "8px"
  control-lg: "12px"
  full: "9999px"
spacing:
  xs: "4px"
  sm: "8px"
  md: "12px"
  lg: "16px"
components:
  button-primary:
    backgroundColor: "{colors.harbor-teal}"
    textColor: "{colors.doorway-ink}"
    typography: "{typography.body}"
    rounded: "{rounded.control-lg}"
    padding: "10px 16px"
    height: "44px"
  device-row:
    backgroundColor: "{colors.mist-surface}"
    textColor: "{colors.doorway-ink}"
    typography: "{typography.title}"
    rounded: "{rounded.control-lg}"
    padding: "14px"
    height: "76px"
---

# Design System: SwarmDrop Mobile

本文件继承根 [DESIGN.md](../DESIGN.md)，只记录原生移动适配。Harbor Teal、状态语义、信任颜色、图标含义和发送入口合同均继承根文档，不能在这里重定义。

## Overview

移动端是“加密工作台”的触控优先表达：直接、紧凑，并能容忍系统中断。原生能力让它在 iOS 和 Android 上自然，但它仍是同一套可信设备产品，不再拥有“门阶”式的第二视觉世界。

## Colors

`mobile/src/global.css` 是运行时来源。`--primary` 是 Harbor Teal 填充，配深色 `--primary-foreground`；`--primary-ink` 是可访问文字形态。亮暗主题均提供 `--success`、`--warning`、`--destructive`、`--info` 及其 `-ink` 变体。

owned 信任用 `primary`，collaborator 用 `muted`，temporary 用 `warning`，blocked 用 `destructive`。在线是连接状态，不是 collaborator 的信任颜色。不得恢复旧蓝色主色、白字 Action Teal 按钮或 collaborator 绿色映射。

## Typography

使用 OS 系统字体：iOS 为 SF Pro，Android 为 Roboto。Title（15px）是设备/列表标题，Body（14px）用于说明和操作，Label（12px）是元数据下限。30px Display 只用于真实传输指标，不能作为装饰标题。

机器值在可用时使用平台等宽字；人类文本和收到的文本使用系统正文。

## Layout

尊重安全区域，使用共享的 4 / 8 / 12 / 16 节奏。主 tab bar 只放持久目的地；发送从设备进入且预选目标。

文本模式中，`文件 / 文本` 选择器位于既有文件来源选择上方。文本使用多行原生编辑器、明确粘贴操作、可见字节数和既有底部操作栏发送。它不是第五个文件来源，也不能增加 Text tab。

## Elevation & Depth

静止表面近乎平面，以边框和极弱阴影区分。Sheet、Dialog、Popover 和 Menu 使用更强的浮层深度。移动端不要求继承桌面/WebGL 或玻璃材质；原生清晰度、电量和 reduced-transparency 优先。

## Shapes

控件和行使用 6–12px 圆角；全圆角只用于紧凑状态/信任 chip。可点控件至少 44 × 44，具备 pressed 反馈，不能依赖 hover。

## Components

### Primary Action and Bottom Action Bar

主操作使用语义主色及其可访问前景。底部操作栏只承载聚焦完成动作（如发送），不能承载被动导航或次级选项。

### Device Row

设备行以紧凑原生布局保留根卡片合同：平台图标、名称、圆点加在线文字、有效元数据、适用的信任/连接信息和发送入口。离线行保持可见，但不能开始发送。

### Text Delivery

仅在用户点击 Paste 后读取剪贴板，仅在用户点击 Copy 后复制收到的文本。权限拒绝、空剪贴板或 native bridge 失败都不能改变编辑器正文，必须保留手动输入。屏幕阅读器标签需宣布目标设备、字节上限、发送可用性和送达结果。

## Do's and Don'ts

### Do:

- **Do** 优先选择原生 Sheet、文本选择、返回行为和安全区布局，而不是模仿桌面材质。
- **Do** 在应用从后台返回时明确展示状态与破坏性确认。

### Don't:

- **Don't** 创建移动专属调色板、信任映射或视觉隐喻。
- **Don't** 在重启后自动重试文本投递，或静默触碰系统剪贴板。
