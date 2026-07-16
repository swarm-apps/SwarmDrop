# Inbox File Preview

## Why

收件箱详情的「打开文件」按钮实际调用的是系统**分享面板**（`Sharing.shareAsync`）——用户点「打开」预期看到内容，看到的却是"发送到微信/存储到文件"，语义错位。查看收到的文件是收件箱的核心动线，目前要么跳出 app 手动找应用，要么（Android 私有目录 + 无处理应用时）根本打不开。

## What Changes

- **「打开文件」真正打开**：iOS 用 QuickLook 系统预览（图片/视频/PDF/Office 文档全覆盖，原生零 UI）；Android 用 `ACTION_VIEW` + content:// 把文件交给用户已装的系统应用。「分享」降级为次级动作（更多操作 sheet）。
- **应用内媒体轻预览**（不跳出 app）：详情页内联图片大图可点开全屏查看（捏合缩放/下滑关闭）；单文件视频在预览位内联播放（原生控制条，可全屏）。
- **降级链**：系统打开无处理应用 → 分享面板 → toast 如实提示，missing 文件照旧标记缺失。
- **明确不做**：自建文档渲染器、通用播放器、多文件场景的逐文件内联预览——系统预览已覆盖，产品定位是传输工具（The Trusted Doorstep），不是文件管理器。

## Capabilities

### New Capabilities

- `mobile-inbox-media-preview`: 收件箱详情的应用内媒体预览——图片全屏查看器与单文件视频内联播放的行为边界（何时渲染、何种交互、何时不渲染）。

### Modified Capabilities

- `mobile-inbox-file-actions`: 「File open/share action」需求变更——主动作从"open or share"（分享面板）改为"open with system preview"（系统打开/预览），分享降级为显式次级动作；降级链与 missing 标记语义保留。

## Impact

- **新依赖**：`react-native-file-viewer`（iOS QuickLook，原生模块，走 new-arch interop）、`expo-video`（视频内联播放，原生模块）、`react-native-image-viewing`（图片全屏查看器，纯 JS）。前两者需要重建 dev client（iOS/Android 各一次）；CI release 每次 `prebuild --clean`，不受影响。
- **代码**：`src/app/inbox/[itemId].tsx`（动作语义拆分 open/share、预览组件）、新增 `src/lib/open-file.ts`（跨平台系统打开）、`isVideoFile` 工具、lingui 双语 catalog。
- **不动**：core/Rust 桥、传输详情页（其「在收件箱查看」链到本页）、offer 阶段（内容先于同意到达违背 E2E 语义，不做缩略图）。
