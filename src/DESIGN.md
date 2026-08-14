---
name: SwarmDrop Desktop
description: 共享 SwarmDrop 设计契约的桌面适配。
colors:
  app-shell: "oklch(0.99 0.001 210)"
  glass-panel: "rgb(255 255 255 / 0.58)"
rounded:
  panel-sm: "18px"
  panel: "24px"
spacing:
  panel: "16px"
components:
  desktop-panel:
    backgroundColor: "{colors.glass-panel}"
    rounded: "{rounded.panel}"
    padding: "{spacing.panel}"
---

# Design System: SwarmDrop Desktop

本文件继承根 [DESIGN.md](../DESIGN.md)，只记录桌面实现约束；跨端语义、token 角色和组件合同均以根文档为准。

## Overview

桌面是完整窗口的工作台。它使用克制的结构玻璃和环境网络层建立纵深，但任务仍以键盘和鼠标高效完成。顶部栏和面包屑提供定位，不在这里引入侧栏或第二套视觉世界。

## Colors

`src/index.css` 是桌面运行时 token 源。`--primary` 是填充，配深色 `--primary-foreground`；`--brand` 是可访问文字/图标形态。状态文字使用 `--*-ink`，不能在浅色表面直接使用原始状态填充色。

## Typography

使用根文档的系统无衬线和机器字角色。桌面可提供快捷键，但快捷键提示和字面值使用等宽字；用户文案和收到的文本仍用正文无衬线字。

## Layout

桌面 shell 使用单一顶部栏：不可点击的品牌标记、节点状态控制和面包屑。发送从设备卡进入且预选目标；文本模式位于既有发送 route 内，不创建常驻页面或悬浮聊天窗。

## Elevation & Depth

`AppAmbientBackground` 提供桌面环境层。玻璃只用于 `--glass-panel-bg`、`--glass-card-bg`、`--glass-control-bg` 等面板级 chrome，并提供 reduced-transparency 平面降级。与 Web 声明同源的 shader/config 必须一起维护。

## Shapes

交互元素使用根控件圆角，结构玻璃使用 `--radius-panel-sm` / `--radius-panel`。可以增加 hover，但键盘焦点必须始终可见，任何关键操作不能只依赖 hover。

## Components

### Navigation

顶部栏是桌面导航 chrome；面包屑表达层级。没有单独产品决策时，不能加常驻 rail。

### Text Delivery

粘贴只能在用户明确点击后经 `src/lib/clipboard.ts` 读取；收件箱文本详情的复制也相同。`Ctrl/Cmd+Enter` 可以提交有效编辑器，但可见的发送按钮仍是主要入口，并防止重复提交。

## Do's and Don'ts

### Do:

- **Do** 使用 `src/index.css` 的语义变量，不直接使用 Tailwind 调色板类。
- **Do** 改动环境动效时一并验证无障碍降级。

### Don't:

- **Don't** 在桌面 WebView 中使用 `navigator.clipboard`。
- **Don't** 对按钮、输入框、编辑器或设备行操作使用玻璃模糊。
