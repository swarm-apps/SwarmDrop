## Why

file-browser 网格(`FileGridView` → `FileCard`)其实早就会画缩略图 —— `item.previewUri` 有值就画 `<Image>`,否则画 lucide 文件类型图标。但这份数据只在**一处**被填:收件箱 scope、仅**图片**、且是**全分辨率**原图路径。于是发送方的网格(挑文件发送时)和所有视频文件都只显示一个通用图标 —— 用户在发送时无法一眼分清哪张照片/哪段视频,收到的视频也只是一块没有播放标识的匿名方块。这是典型的"渲染早就在、只差喂数据",补上它是高可见度的打磨收益,且**不需要任何协议或 core 改动**。

## What Changes

- 为移动端 file-browser 网格加一条跨 scope 的缩略图管线:
  - **发送 scope**(用户即将发送的文件):用 app 本就持有的本地 `file://` 源,显示图片**和**视频缩略图。
  - **收件箱 scope**(已接收、已完成的文件):显示图片**和**视频缩略图;替换当前"仅图片、全分辨率"的 `inboxPreviewUri`。
- 给 `FileBrowserItem` 增加 `localUri?` 字段(本设备上存在时的 `file://` 路径),在 `fromSelectedFiles` / `fromInboxFiles` 适配器里填充。**移除** `fromInboxFiles` 上临时的 `previewUriFor` 回调。
- 新增 `useFileThumbnail(item)` hook 解析要显示的缩略图:图片直接渲染(由图片层负责降采样),视频异步生成带缓存的首帧海报;两者在缺失/失败时都回退到类型图标。
- 把 `FileCard` 网格图片从 React Native 原生 `<Image>` 迁移到 **`expo-image`**(降采样 + 内存/磁盘缓存 + EXIF 方向自动纠正),并在视频缩略图上叠一个小播放角标。
- 把图片/视频扩展名判定收敛到一个 `media-type` 模块,替换当前三处互相不一致的副本(`file-icon.ts`、`inbox/[itemId].tsx`、`inbox-list.tsx`)。
- 新增依赖:`expo-image`(~56.0.11)与 `expo-video-thumbnails`(~56.0.3)。两者都是原生模块 → 需要重新原生构建(dev client / 构建产物;非纯 Expo Go)。
- **明确不在本轮范围(有意推迟,记为后续)**:transfer / offer scope;接收方下载前预览(需要改 Rust 协议以携带发送方缩略图);以及桌面 app(独立仓库,见 Impact)。

## Capabilities

### New Capabilities

- `file-browser-thumbnails`: 移动端 file-browser 网格模式的图片与视频缩略图渲染与生成能力,范围限定 send(本地源文件)与 inbox(已接收完成文件),含本地文件门控、带缓存的异步视频海报生成,以及图标回退契约。

### Modified Capabilities

<!-- 无。mobile-inbox-media-preview 只管单文件收件箱详情页的查看器/播放器,不涉及多文件网格缩略图;mobile-file-browser(经 unify-mobile-file-browser 在途)在需求层不受影响。 -->

## Impact

- **新增依赖**:`expo-image ~56.0.11`、`expo-video-thumbnails ~56.0.3`(均在 `bundledNativeModules.json` 里按 SDK 56 pin)。两端(arm64)需重新原生构建。
- **受影响代码(移动端)**:
  - `src/components/file-browser/types.ts` —— 增加 `localUri?`。
  - `src/components/file-browser/adapters.ts` —— `fromSelectedFiles` + `fromInboxFiles` 填 `localUri`;去掉 `previewUriFor`。
  - `src/components/file-browser/file-card.tsx` —— 迁移到 expo-image、接 `useFileThumbnail`、视频播放角标。
  - 新增 `src/components/file-browser/media-type.ts` 与 `src/components/file-browser/use-file-thumbnail.ts`。
  - `src/app/inbox/[itemId].tsx` —— 移除 `inboxPreviewUri`;改用集中的 `media-type`。
  - `src/components/inbox/inbox-list.tsx` —— 改用集中的 `media-type`。
  - `src/app/e2e/file-browser.tsx` —— fixture 断言的是 `previewUri`,需改为 `localUri` 契约。
  - 为 `expo-image` 注册 nativewind `cssInterop`。
- **后续独立 change(预先文档化在 design.md,避免调研成果丢失)**:
  1. 桌面**图片**缩略图镜像(Tauri `image` crate + 磁盘缓存)—— 桌面仓库。
  2. 桌面**视频**缩略图(ffmpeg sidecar;每平台 +40–80 MB、GPL/LGPL、macOS 需公证)—— 桌面仓库,取决于是否接受该包体成本。
  3. 移动端 **offer scope** 预览(发送方把缩略图塞进传输 offer)—— 需要 Rust core/协议改动。
