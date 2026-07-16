# Design: Inbox File Preview

## Context

- 收件箱详情（`src/app/inbox/[itemId].tsx`）现状：
  - `openOrShareFile` 对单文件主按钮和多文件 FileRow 都走 `Sharing.shareAsync`（分享面板），文案却叫「打开/分享」。
  - 已有 Tier-0 轻预览：`ImagePreview`（内联图片大图，`file://` + `isImageFile` 才渲染）、`TextExcerptCard`（文本摘录）。「大预览只给真内容」原则已确立（见 `dev-notes` 与 DESIGN.md）。
  - `isImageFile` / `isVideoFile` / `fileIcon` 工具已在本文件内存在（`VIDEO_EXTENSIONS` 齐备），无需新建。
- `localPath` 有两种形态：**`file://`**（Android 私有目录默认接收位置 + iOS 全部）与 **`content://`**（Android 用户自定义 SAF 目录）。
- 栈：Expo SDK 56 / RN 0.85（new arch）。`expo-file-system` 主入口是新 API（`File`/`Paths`），`getContentUriAsync` 只在 `expo-file-system/legacy`。
- 已有工具：`startViewIntent`（`src/core/saf-intent.ts`，ACTION_VIEW + grant flags）。

## Goals / Non-Goals

**Goals:**
- 「打开」按下后用户**看到文件内容**（系统预览/系统应用），而不是分享面板。
- 图片与单文件视频**不跳出 app**：图片全屏可缩放，视频内联可播。
- 打不开时有确定性的降级反馈，missing 语义不回退。

**Non-Goals:**
- 自建 PDF/Office 渲染、通用播放器（格式长尾，偏离产品定位）。
- 多文件记录的逐文件内联预览（无大预览位；逐文件走系统打开）。
- offer 阶段缩略图（内容先于同意到达，违背 E2E 接受语义，需动协议）。
- 传输详情页的文件预览（其「在收件箱查看」已链到本页）。

## Decisions

### D1: iOS「打开」= QuickLook（`react-native-file-viewer`）
- QLPreviewController 原生覆盖图片/视频/PDF/Office/文本等，零 UI 开发，是 iOS「查看文件」的系统语言。
- 备选：维持分享面板（语义错位，否）；WebView 渲染（格式长尾，否）；自写 local expo module 直呈 QLPreviewController（~50 行 Swift，作为 **兜底方案**，见 R1）。
- 传参：RNFV 需要**解码后的绝对路径**——`decodeURIComponent(localPath.replace(/^file:\/\//, ""))`（中文文件名是 percent-encoded 的）。

### D2: Android「打开」= 自拼 `ACTION_VIEW`，不用 RNFV 的 Android 端
- RNFV Android 只认文件路径，对 SAF `content://` 无能为力；而我们已有 `startViewIntent` 工具。
- 路径归一：`content://` 直接用；`file://` 经 `getContentUriAsync`（`expo-file-system/legacy`）转 content://（Expo FileProvider 已配置，expo-sharing 同源）。
- Intent 不显式 `setType`：resolver 会通过 `ContentResolver.getType()` 向 provider 查 MIME 完成匹配，避免 type+data 同设的兼容坑。grant flags 沿用 `startViewIntent` 默认（READ + NEW_TASK）。
- 降级链：`startActivityAsync` 抛错（无处理应用）→ `Sharing.shareAsync` → 仍失败 toast。missing 检测（`ensureAvailable`/`isMissingFileError`）在最外层保留。

### D3: 图片全屏 = `react-native-image-viewing`（纯 JS）
- 纯 JS（RN Modal + 手势），零原生成本零重建风险；捏合缩放/双击/下滑关闭齐备。
- 入口：`ImagePreview` 内联大图包 Pressable，点击开全屏；单图（`previewImageFile` 本就限定单文件）。
- 备选：Galeria（原生依赖，共享元素动画——超出"简单做一下"）；自写 gesture-handler 缩放（重复造轮子）。

### D4: 视频内联 = `expo-video`（官方，SDK 配套版本）
- 单文件视频复用大预览位：`previewVideoFile` 与 `previewImageFile` **镜像判定**（单文件、`!missing`、`file://`、`isVideoFile`），渲染 `VideoView`（原生控制条 + 系统全屏按钮），不自动播放。
- `file://` only（与图片对称）：SAF `content://` 视频 ExoPlayer 理论可放但 iOS 无此形态，首版不放开，SAF 用户走「打开」（系统播放器）。见 Trade-offs。

### D5: 动作语义重排
- `DetailActionBar` 主按钮：「打开/分享」→「打开」（icon `Share2` → `Eye`）。
- 「分享」降级为更多操作 sheet 的显式行（仅单文件且非 missing 时渲染），走原 `shareAsync` 路径。
- 多文件 `FileRow` tap → 系统打开该文件；逐文件分享舍弃（系统打开后的目标应用自带分享；需求真实存在时再回加行尾按钮）。
- 新增 `src/lib/open-file.ts`：`openFileWithSystem(localPath)` 封装 D1+D2，UI 层只管降级编排。

## Risks / Trade-offs

- [R1] RNFV 2.1.5 是 old-arch 模块，在 RN 0.85 靠 interop 层运行，可能编译/运行失败 → 验证放在任务最前（装依赖后先跑 `expo run:ios` 冒烟）；失败即切兜底：`modules/quick-look/` 本地 expo-module（Swift 直呈 QLPreviewController），接口保持 `open(path)` 不变，上层零改动。
- [R2] RNFV 的 Android manifest 会合并进来（provider authority `${applicationId}.provider`）→ 我们不调用其 Android 端，authority 与 Expo 系不冲突；若 merge 报错，`tools:node="remove"` 摘除。
- [R3] `getContentUriAsync` 在 legacy 子路径，未来 SDK 可能移除 → 调用处集中在 `open-file.ts` 单点，届时换新 API 等价物。
- [R4] Android 设备无处理应用（模拟器常见，如 .bin/.txt）→ 降级链兜底到分享面板；这正是保留分享路径的原因。
- [R5] 视频 `file://` only：SAF 自定义目录用户无内联视频 → 可接受（有「打开」系统播放器），后续需要时单独放开 content:// 并验证权限持久性。
- [R6] QuickLook 对不可预览格式显示"不支持"占位 → 可接受，占位内自带分享按钮，仍优于直接弹分享面板的语义错位。

## Migration Plan

纯前端改动，无数据/协议迁移。两个原生依赖需要重建 dev client（`expo run:ios` / `expo run:android`，本地 android/ 增量）；CI release 每次 `prebuild --clean` 自动包含。回滚 = revert 单个 commit。

## Open Questions

- 无阻塞项。视频 content:// 放开与多文件逐文件分享回加，留待真实需求触发。
