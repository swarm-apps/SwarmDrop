## 1. 依赖与原生构建

- [x] 1.1 `npx expo install expo-image expo-video-thumbnails`,确认解析到 SDK 56 pin(expo-image ~56.0.11、expo-video-thumbnails ~56.0.3)
- [ ] 1.2 两端(iOS + Android arm64)重新原生构建 / prebuild,确认原生模块链接、app 能起 —— **待真机/模拟器构建(需 booted device)**

## 2. 媒体类型判定收敛

- [x] 2.1 新增 `src/components/file-browser/media-type.ts`:导出 `isImageFile(name)`、`isVideoFile(name)`、`IMAGE_EXTENSIONS`、`VIDEO_EXTENSIONS`(合并去重,补齐 heif/avif/tiff 等缺项)
- [x] 2.2 `file-icon.ts` 改用 `media-type` 的扩展名集合(icon 分组不再自带一份)
- [x] 2.3 `src/app/inbox/[itemId].tsx` 移除本地 `IMAGE_EXTENSIONS`/`VIDEO_EXTENSIONS`/`isImageFile`/`isVideoFile`,改 import `media-type`
- [x] 2.4 `src/components/inbox/inbox-list.tsx` 的图片/视频判定改用 `media-type`

## 3. 数据契约:localUri

- [x] 3.1 `types.ts` 给 `FileBrowserItem` 增加 `localUri?: string`
- [x] 3.2 `adapters.ts`:`fromSelectedFiles` 填 `localUri = file.sourceId`
- [x] 3.3 `adapters.ts`:`fromInboxFiles` 填 `localUri = file.missing ? undefined : file.localPath`(file:// 门控),并移除 `previewUriFor` 参数
- [x] 3.4 `src/app/inbox/[itemId].tsx` 移除 `inboxPreviewUri` 及其在 `fromInboxFiles(...)` 调用处的传参

## 4. 缩略图生成管线

- [x] 4.1 新增视频缩略图缓存模块(`video-thumbnail-cache.ts`):`<Paths.cache>/video-thumbs/` 目录、按稳定 id keying、`getThumbnailAsync(uri,{time:1000,quality:0.7})` → move 到 `<key>.jpg`、`dest.exists` 短路、并发闸(上限 3)、失败 try/catch 返回 null
- [x] 4.2 新增 `src/components/file-browser/use-file-thumbnail.ts`:无 localUri/非 file:// → undefined;图片 → 直接返回 localUri;视频 → 走 4.1 缓存模块异步解析,期间 undefined
- [x] 4.3 hook 内维护会话级 `Map<id, uri>` 记忆,避免 cell 重挂载重复触发生成

## 5. FileCard 迁移到 expo-image

- [x] 5.1 用 expo-image `style` prop 铺满 cell(避开 nativewind `className` 对 expo-image 失效的坑,无需全局 cssInterop)
- [x] 5.2 `file-card.tsx`:RN `<Image>` → expo-image `<Image>`,`resizeMode="cover"` → `contentFit="cover"`,加 `recyclingKey={item.id}`、`cachePolicy="memory-disk"`、`transition={0}`、保留默认 `allowDownscaling`
- [x] 5.3 `file-card.tsx`:`item.previewUri` → `useFileThumbnail(item)`;保留 `onError` → 图标回退(HEIC/解码失败兜底)
- [x] 5.4 `file-card.tsx`:视频缩略图叠加播放角标(`isVideoFile(item.name)` 且有海报时显示居中 ▶ 覆盖层)

## 6. 测试与验证

- [x] 6.1 更新 `src/app/e2e/file-browser.tsx` fixture:`previewUri` → `localUri` 契约(`fromInboxFiles` 去掉第三参 + 断言改 `localUri`)
- [x] 6.2 强化 fixture 断言(项目无 jest,用既有 `fixtureAssertionsPass`):`fromSelectedFiles` 填 `localUri`、inbox 图片填、缺失文件不填
- [x] 6.3 `biome ci` + `tsc --noEmit` 通过
- [ ] 6.4 真机(emulator)验证:发送 scope 图片缩略图 + 视频海报;收件箱多文件网格 图片 + 视频;长滚动无卡顿/无闪旧图;HEIC / 大图 / 编解码失败回退图标 —— **待真机/模拟器**
- [x] 6.5 确认 transfer / offer scope 仍为图标(adapters 只在 send/inbox 设 localUri)

## 7. 收尾

- [x] 7.1 `openspec validate add-file-browser-thumbnails --strict` 通过
- [x] 7.2 变更集自查:无遗留 `previewUri` 引用、扩展名表只剩 `media-type.ts` 一处
