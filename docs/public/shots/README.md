# 首页产品截图

官网首页（`app/(home)/page.tsx`）用的产品图，**全部是真实界面的现场截图**，不是渲染的示意图。

| 文件 | 尺寸 | 来源 | 抓法 |
|---|---|---|---|
| `desktop-devices.png` | 1680×920 | 桌面端 `/devices`（暗色） | `pnpm tauri dev` 起真机 → 与浏览器端真配对 → `manage_window` 调到 1680×1000 → `webview_screenshot` → ffmpeg 裁到 920 高 |
| `desktop-pairing.png` | 1680×960 | 桌面端「添加设备 › 展示邀请」（暗色） | 同上，不裁 |
| `web-devices.png` | 2800×1300 | 浏览器端 `/app/devices`（暗色） | `agent-browser set viewport 1400 880 2` → `screenshot` → ffmpeg 裁到 1300 高 |

桌面端的 `webview_screenshot` 出的是**逻辑像素**（dpr=1），所以那两张靠「把窗口开大」拿分辨率，
不是靠 @2x；浏览器端能直接设 `deviceScaleFactor=2`。

## 三条规矩

1. **不许用画的。** 仓库里那支 `public/hero/swarmdrop-hero.mp4` 是 Remotion 渲染的**模拟界面**
   （不是真产品），配色还停在旧的深蓝身份，文案里带着一个没转义的 `\n`——它不进首页。
   div 拼的假界面同理。

2. **不许摆拍出不存在的状态。** 图里的在线设备、连接方式、延迟、文件名都来自一次真实的
   配对与传输。要拍「有内容」的界面就去真的产生内容，不要改代码塞假数据。
   唯一允许的干预是**遮掉开发期产物**——截图前注入
   `nextjs-portal{display:none}` 关掉 Next Dev Tools 浮标，那不是产品 UI。

3. **@2x，且别忘了重拍。** 界面改了图就过期了。尤其是设备卡：2026-08-06 那次改动
   （连接徽标去掉传输名、发送按钮回到徽标行）让这两张图整个作废过一次。
   改了 `device-card.tsx` / `connection-badge.tsx` 就回来重拍。

## 尺寸

`next.config.mjs` 里 `images: { unoptimized: true }`（静态导出没有优化服务），
所以进仓的就是最终产物——**下载体积等于文件体积**。截完过一遍 `pngquant` 或
`oxipng`，控制在单张 400 KB 以内。
