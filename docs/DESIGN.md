---
name: SwarmDrop Web
description: 共享 SwarmDrop 设计契约的浏览器适配。
colors:
  brand-solid: "oklch(0.583 0.105 177.1)"
  brand-ink: "oklch(0.516 0.093 178.2)"
  app-surface: "oklch(1 0 0)"
rounded:
  control: "10px"
  panel-sm: "18px"
  panel: "24px"
spacing:
  in-panel: "16px"
  panel: "24px"
components:
  web-panel:
    backgroundColor: "{colors.app-surface}"
    textColor: "{colors.brand-ink}"
    rounded: "{rounded.panel}"
    padding: "{spacing.panel}"
---

# Design System: SwarmDrop Web

本文件继承根 [DESIGN.md](../DESIGN.md)，描述 `docs/app/app` 中的浏览器应用；它不重设周围文档站点，也不重定义跨端语义。

## Overview

Web 是文档站点内的一间应用房间。它与桌面共享加密工作台语言，但浏览器需要可见的返回入口和常驻应用导航。页面要像应用，而不能与 Fumadocs 文档 chrome 竞争。

## Colors

`docs/app/global.css` 是 Web 运行时映射。无前缀语义 token 服务应用区；Fumadocs 的 `--color-fd-*` token 仍属于文档站点合同。`--brand-solid` 是 Harbor Teal 填充，`--brand` 是可访问文字形态；不能通过改写 Fumadocs token 混合两套 token。

## Typography

使用根文字角色。peer ID、速度、字节数和配对码等值用等宽字；文本 Inbox 内容是可选择、保留空白的正文。

## Layout

应用 rail 包含设备、Inbox 和设置。发送与传输是设备子页：保持设备激活态，并在页面标题中表达父级。窄屏退化为既有底部导航；应用导航始终在应用 shell 内，不能落入 Fumadocs 页面布局。

文本发送从已选设备开始，在文件/文本选择器中保持目标。最近发送只服务该发送视图，收到的文本仍属于 Inbox。不能添加顶级 Text、Send、Transfer 或 Outbox 导航项。

## Elevation & Depth

Web ambient canvas 必须延迟加载，并遵守既有 DPR 和帧率约束。玻璃面板可以折射环境层，但只有结构容器能模糊。亮色主题必须跨动画帧保持文字对比度；reduced-motion 冻结 canvas，reduced-transparency 则移除对环境层/玻璃的依赖。

## Shapes

通过 `--radius`、`--radius-panel-sm`、`--radius-panel` 使用根圆角分类。每个响应式层级都保持 44 × 44 命中区和可见键盘焦点。

## Components

### Navigation

品牌标记链接营销主页。rail/bottom-nav 状态和可访问标签来自共享导航模型。浏览器 rail 是桌面面包屑 shell 的有意例外，不能反过来成为桌面的先例。

### Text Delivery

剪贴板只能在安全上下文的用户手势内读写。Clipboard API 不可用或被拒绝时，保留手动编辑和浏览器原生粘贴；不能清空现有文本或假装粘贴成功。

## Do's and Don'ts

### Do:

- **Do** 使用应用语义 token，并把 Fumadocs token 限制在文档 UI。
- **Do** 发布前延迟加载并节流装饰性的 WebGL 工作。

### Don't:

- **Don't** 在路由加载、焦点或后台定时器中读取剪贴板。
- **Don't** 通过 Web 专属导航破坏根发送入口合同。
