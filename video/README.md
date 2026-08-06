# SwarmDrop 官网 Hero 视频

这个目录是独立的 Remotion 成片工程。它只负责制作官网素材，官网运行时不加载 Remotion。

> **当前官网没有引用它**（2026-08-06）。首页改用**真实产品截图**（`docs/public/shots/`，
> 现场从运行中的桌面端与浏览器端截的），旧成片与封面图已从 `docs/public/hero/` 删除——
> 7.1 MB 无人引用却每次都要部署到 GitHub Pages。
>
> 重新启用前**必须先重录 `HeroLoop`**：它画的是**模拟界面**，用的还是早已废弃的深蓝身份配色，
> 直接渲染出来贴上官网就是拿假界面当产品图。`docs/public/shots/README.md` 里那条
> 「不许用画的」写的就是这件事。

桌面 / 移动端 Demo 的事件驱动后期方案见
[`e2e/desktop/demo-postproduction-design.md`](../e2e/desktop/demo-postproduction-design.md)。

## 命令

```bash
# 打开 Remotion Studio，预览 HeroLoop
pnpm studio

# 输出供 GitHub Pages 播放的 MP4
pnpm render:hero

# 输出视频封面图
pnpm render:poster

# 静态检查
pnpm lint
```

成片会写入 `../docs/public/hero/`。**注意那个目录现在是空的、也没有页面引用它**（见开头），
渲染出来不会自动出现在官网上。原始录屏素材应保存在 `public/footage/`，不要把未经裁剪的
长录屏提交到仓库。

## 设计约束

- `HeroLoop` 是 1920 × 1080、30 fps、20 秒的无声循环画面。
- 官网标题和下载按钮已在页面左侧呈现；视频先呈现蜂群互联，再用三张功能页介绍跨网络、端到端加密与本地 MCP 调度。
- 所有时间变化必须由 `useCurrentFrame()` 驱动，不能依赖 CSS 动画或 transition。
