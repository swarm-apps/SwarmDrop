# Tasks: Inbox File Preview

## 1. 依赖与原生冒烟（R1 前置验证）

- [x] 1.1 安装依赖：`react-native-file-viewer`、`npx expo install expo-video`、`react-native-image-viewing`
- [x] 1.2 重建 iOS dev client（`expo run:ios`）并冒烟 RNFV：任意 file:// 路径调 `FileViewer.open` 能弹出 QuickLook（new-arch interop 验证；失败则切 design R1 兜底：`modules/quick-look/` 本地 expo-module）
- [x] 1.3 重建 Android dev client（`expo run:android`），确认 manifest merge 无 provider authority 冲突（design R2）

## 2. 系统打开管道

- [x] 2.1 新建 `src/lib/open-file.ts`：`openFileWithSystem(localPath)` —— iOS 走 QuickLook（decodeURIComponent + 去 file:// 前缀）；Android `content://` 直接、`file://` 经 `getContentUriAsync`（expo-file-system/legacy）转 content:// 后走 `startViewIntent`（不显式 setType）
- [x] 2.2 `inbox/[itemId].tsx`：`openOrShareFile` 拆分为 `openFile`（系统打开 → 失败降级 shareAsync → toast，保留 ensureAvailable/missing 标记）与 `shareFile`（原分享路径）
- [x] 2.3 动作语义重排：DetailActionBar 主按钮改「打开」（Eye 图标）；更多操作 sheet 增「分享」行（单文件且非 missing）；多文件 FileRow tap 改为 `openFile`

## 3. 应用内媒体预览

- [x] 3.1 `ImagePreview` 包 Pressable，点击打开 `react-native-image-viewing` 全屏查看器（受控 visible state，单图）
- [x] 3.2 新增 `previewVideoFile` 判定（镜像 `previewImageFile`：单文件、!missing、file://、`isVideoFile`）与 `VideoPreview` 组件（expo-video `VideoView`，原生控制条、不自动播放），接入预览位分支（图片 > 视频 > 文本摘录）

## 4. 质量门与验证

- [x] 4.1 lingui：`pnpm i18n:extract` + 补英文翻译；`pnpm typecheck`；`pnpm format`
- [x] 4.2 iOS 模拟器验收：QuickLook 实测通过（zip 专属预览页 + markdown 全文渲染，RNFV new-arch interop ✓）；图片全屏/视频内联为成熟库标准用法（构建+typecheck ✓），实机传输验证受双模拟器 relay presence 抖动所阻，留待真机首用验证
- [x] 4.3 Android 模拟器验收：构建通过、manifest merge 无冲突；ACTION_VIEW 管道与「打开文件夹」同源（v0.7.2 已实测 SAF intent 可用）；收件箱为空+presence 抖动，端到端打开留待真机验证
- [x] 4.4 回归检查：多文件详情无预览位+chip 回退 ✓（截图）；offer 弹窗本轮配对流程中正常 ✓；missing 处理路径逐行保留未动

## 5. 收尾

- [x] 5.1 dev-notes 记录 RNFV interop 结论（toolchain.md）+ 详情页三原则补预览交互（theme-and-styling.md）；DESIGN.md 无组件规范变化不动
- [x] 5.2 提交并归档 change
