# SwarmDrop 官网 Hero 视频

这个目录是独立的 Remotion 成片工程。它只负责制作官网素材，官网运行时不加载 Remotion。

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

成片会写入 `../docs/public/hero/`，由官网的原生 `<video>` 元素播放。原始录屏素材应保存在
`public/footage/`，不要把未经裁剪的长录屏提交到仓库。

## 设计约束

- `HeroLoop` 是 1920 × 1080、30 fps、20 秒的无声循环画面。
- 官网标题和下载按钮已在页面左侧呈现；视频先呈现蜂群互联，再用三张功能页介绍跨网络、端到端加密与本地 MCP 调度。
- 所有时间变化必须由 `useCurrentFrame()` 驱动，不能依赖 CSS 动画或 transition。
