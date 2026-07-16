## Context

共享 file-browser(`src/components/file-browser/`)经 `FileGridView` → `FileCard` 渲染网格。`FileCard` 已经在 `previewUri` 有值时画 `<Image source={{uri: item.previewUri}} resizeMode="cover">`,否则画 lucide 类型图标。今天 `previewUri` 只在一个适配器里被填 —— `fromInboxFiles`,通过注入的 `previewUriFor` 回调(`src/app/inbox/[itemId].tsx` 里的 `inboxPreviewUri`)—— 且只对**图片**、给 `<Image>` 的是**全分辨率**原图路径。发送 scope 网格(`fromSelectedFiles`)和所有视频都只显示通用图标。

代码里已确认的约束:
- 发送 scope 的 item 一定在 `sourceId` 里带可渲染的 `file://`(document picker 用 `copyToCacheDirectory: true`、相册 picker 返回 file uri、目录选择用 expo-fs uri、share-intent 拷贝到本地路径 —— Android 的 `content://` 在源头就被拷成 `file://`)。见 `src/core/file-access.ts` + `src/core/share-intent.ts`。
- 收件箱 item 在非缺失时于 `MobileInboxFileEntry.localPath` 带 `file://`。
- 适配器(`adapters.ts`)是纯同步函数;`FileBrowserItem` 是纯同步记录。
- 网格用 `@shopify/flash-list`(屏内)/ `BottomSheetFlatList`(sheet 内),含 cell 回收。
- Rust core / 桥接绑定里**不存在**任何 thumbnail/MIME/preview 字段 —— 本 change 完全留在 RN 侧。
- 技术栈:Expo SDK 56.0.12、RN 0.85.3、React 19.2.3、New Architecture。`expo-image` / `expo-video-thumbnails` **未装**;`expo-video ~56.1.4` 与 `expo-file-system ~56.0.8`(新 File/Directory/Paths API)已装。

## Goals / Non-Goals

**Goals:**
- **send** 与 **inbox** scope 的网格模式图片**和**视频缩略图。
- 一条跨 scope 的管线;适配器保持纯净;异步生成 + 缓存隔离在单个 hook 里。
- 有界的内存/CPU:图片降采样解码、并发受限 + 带缓存的视频海报、稳健的图标回退。
- 收敛当前分散在三处的媒体类型(图片/视频扩展名)判定。

**Non-Goals:**
- transfer scope(实时进度)与 offer scope(下载前)缩略图 —— 保持图标。
- 接收方下载前预览(需要改 Rust 协议以携带发送方缩略图)。
- 桌面 app(独立仓库 `/Volumes/yexiyue/SwarmDrop`)—— 记为下方后续 change。
- 收件箱**详情页**的全屏/内联媒体预览 —— 那是 `mobile-inbox-media-preview`,本轮不动。

## Decisions

### D1 —— `localUri` 字段 + `useFileThumbnail` hook(保持适配器纯净)

给 `FileBrowserItem` 加 `localUri?: string`。适配器在本地文件存在处填充:
- `fromSelectedFiles` → `localUri = file.sourceId`
- `fromInboxFiles` → `localUri = file.missing ? undefined : file.localPath`(并**移除** `previewUriFor` 参数)
- `fromProjection` / `fromOfferFiles` → 不填

新增 `useFileThumbnail(item)` hook 解析要显示的 uri:图片 + `localUri` → 直接返回 `localUri`(图片层负责降采样);视频 + `localUri` → 异步带缓存海报(见 D3);否则 → `undefined`(图标)。`FileCard` 改用该 hook,不再读 `item.previewUri`。

**为什么**:异步生成不能塞进纯同步适配器。单个 hook 让缩略图在所有设置了 `localUri` 的 scope 上统一工作,契合在途 `unify-mobile-file-browser` 的"统一"方向,并能删掉 inbox 专用的 `previewUriFor` 特例。备选方案 —— 保留 `previewUri` 在每个页面里按 scope 解析 —— 否决:逻辑分散、保留 inbox 特例、且无处安放视频异步/缓存路径。

**注**:`previewUri` 实际被 `localUri` 取代为 item 级输入;`src/app/e2e/file-browser.tsx` 里断言 `previewUri === "file:///fixture/photo.jpg"` 的 fixture 必须迁到 `localUri` 契约。

### D2 —— 图片渲染用 `expo-image`

安装 `expo-image`(解析到 **~56.0.11**,`bundledNativeModules.json` 里的 SDK 56 pin;**不要**取 npm `latest` = 57.0.0)。迁移 `FileCard` 网格图片:
- `resizeMode="cover"` → **`contentFit="cover"`**(expo-image 会静默忽略 `resizeMode`)。
- 加 **`recyclingKey={item.id}`** —— FlashList 回收下必需,否则被复用的 cell 在新图解码完成前会显示上一个 item 的缩略图。
- `cachePolicy="memory-disk"`、`transition={0}`(非零 transition + 回收会导致 crossfade / 占位尺寸错乱 —— expo/expo #22516、#22206)。
- 保留 `allowDownscaling`(默认开):把 12MP 照片直接解码到 ~96px cell,而非每 cell 一个 ~48MB ARGB 位图(避免 expo/expo #26781 的 OOM)。
- nativewind 默认**不**通过 `className` 给 expo-image 的 `<Image>` 上样式 —— 要么 `cssInterop(Image, { className: 'style' })` 注册一次,要么用 `style` prop。否则现有 `className="size-full"` 会静默失效。

**为什么优于原生 RN `<Image>`**:内置降采样 + 内存/磁盘缓存 + 自动 EXIF 方向(Glide/SDWebImage)—— 正是滚动网格里大量大图所需;原生 `<Image>` 一样都没有,且正是 OOM 路径。备选 —— 用 `expo-image-manipulator` 预生成小图 —— 否决:更重(多一个依赖、要自己管缓存)却不比 expo-image 内置降采样多任何收益。

### D3 —— 视频海报用 `expo-video-thumbnails`

安装 `expo-video-thumbnails`(**~56.0.3**,SDK 56 pin —— 文档里"SDK 56 移除"的横幅是陈旧的;它有发布且在 `bundledNativeModules.json` 里 pin)。`getThumbnailAsync(localUri, { time: 1000 /* 毫秒 */, quality: 0.7 })` → `{ uri: file://… }`,由同一个 expo-image `<Image>` 渲染。

缓存策略(在 `useFileThumbnail` / 一个小模块里):
- 缓存目录 `<Paths.cache>/video-thumbs/`,用新的 expo-file-system `File`/`Directory`/`Paths` API(`exists`/`create` 同步,`move`/`copy` 异步)。
- 按**稳定 id**(`item.id`,或 `hash(path + mtime)`)keying,把生成文件 move 到 `<id>.jpg`,`dest.exists` 就短路 —— 否则 `getThumbnailAsync` 每次调用都写一个新临时文件,缓存无界增长。
- **并发闸**(上限 ~2–3)防止快速滚动引发解码风暴。生成在 `FileCard` 挂载时触发(FlashList 只挂载可见 + overscan 的 cell),因此无需额外接 `onViewableItemsChanged`。
- `time: 1000`(毫秒 —— expo-video 用秒,别混)避开黑首帧。`try/catch` → 返回 `null` → 编解码失败(HEVC/HDR/异常容器)时回退图标。

**为什么优于 expo-video 内置的 `generateThumbnailsAsync`**:内置返回的是 `VideoThumbnail`(`SharedRef<'image'>`),**不是** uri —— 只能被 expo-image 渲染或用 `expo-image-manipulator` 转成文件,且需要每源一个 live `VideoPlayer`(AVPlayer/ExoPlayer),对多 cell 网格太重且没有免费磁盘缓存。`expo-video-thumbnails` 一个便宜依赖就直接产出带缓存的 uri。属于要在 SDK 57+ 复查的技术债(Expo 长期方向是 `generateThumbnailsAsync`)。

### D4 —— 收敛媒体类型判定

新增 `src/components/file-browser/media-type.ts`,导出 `isImageFile(name)`、`isVideoFile(name)` 与规范的 `IMAGE_EXTENSIONS` / `VIDEO_EXTENSIONS`。替换三处互相不一致的副本:`file-icon.ts`(icon 分组 —— 缺 `heif`/`avif`/`tiff`)、`inbox/[itemId].tsx`、`inbox-list.tsx`。hook + `FileCard` 播放角标判定 + 图标选择全部从这一处读。

**为什么**:当前三处副本已经在"哪些扩展名算数"上不一致;缩略图路径又加了第四个消费者,绝不能再分叉。

### D5 —— scope 边界:仅 send + inbox

只有 `fromSelectedFiles` 与 `fromInboxFiles` 设 `localUri`;transfer/offer 保持仅图标。transfer scope 焦点是状态/进度;接收方在传输中的文件是半写状态(不宜生成缩略图);offer scope 根本没有本地字节。

### D6 —— 桌面是独立后续(调研结论存档于此)

桌面 app 是另一个仓库,且镜像了这套 file-browser(`file-card.tsx` 的 `<img>`、inbox 的 `convertFileSrc`)。它成为独立 change;调研出的做法在此记录以免丢失:

- **桌面图片**(后续 A):`image` crate(`image = { version = "0.25", default-features = false, features = ["jpeg","png","webp","gif"] }`)在一个 `#[tauri::command]` 里离线程解码一次、降采样到 ~320px webp、按 `path+mtime` keying 写进 `app_cache_dir`、返回缓存路径,前端用现有 `convertFileSrc` `<img loading="lazy" decoding="async" style="object-fit:cover">` 渲染。首版可先纯 `<img>` + CSS(已能跑);(a)→(c) 只是换 `src`。**必须**把 `$APPCACHE/**/*` 加进 `app.security.assetProtocol.scope`,否则 `convertFileSrc(cachePath)` 会被 403。
- **桌面视频**(后续 B):**ffmpeg sidecar** —— 按 target-triple 打包静态 ffmpeg(`bundle.externalBin` + `tauri-plugin-shell`),`#[tauri::command]` 跑 `ffmpeg -ss 0.1 -i <path> -frames:v 1 -vf scale=... -y out.webp` 写进同一缓存。纯 webview `<video>`+canvas 被否决(asset:// 不提供 Range → WKWebView 加载不了 `<video>`;WebView2 缺 HEVC;WebKitGTK 缺 GStreamer 编解码器 —— 每台机器不确定)。`ffmpeg-next`/`video-rs` crate 被否决(build 期要系统 FFmpeg 开发库 → CI 痛苦)。`tauri-plugin-video-thumbnail`/`thumbnailer` crate 被否决(实际解不了视频)。**成本门槛**:每平台 +40–80 MB、GPL/LGPL 署名、macOS 要给 sidecar 签名公证 —— 这正是桌面视频作为有意分离、按成本门控的独立 change 的原因。
- **offer scope 预览**(后续 C):发送方把小缩略图塞进传输 offer —— Rust core/协议改动;两端都受益;是接收方下载前预览的唯一路径。

## Risks / Trade-offs

- **旧 Android 上的 HEIC** → expo-image 委托 OS 解码器;Android 到 API 29(Android 10)才加系统级 HEIC/HEIF。iOS 端发来的 `.heic` 在 Android 9 上会失败。→ 缓解:保留 `FileCard` 现有 `onError` → 图标回退(对 expo-image 一样生效);可选后续:用 `expo-image-manipulator ~56.0.19` 有条件转码。
- **视频编解码失败**(HEVC/HDR)→ `getThumbnailAsync` 在某些设备上可能抛错/返回空。→ 缓解:`try/catch` → `null` → 图标;绝不假设成功。
- **缓存增长** → `getThumbnailAsync` 每次调用写新临时文件。→ 缓解:稳定 id keying + `dest.exists` 短路(D3)。
- **解码风暴 / 卡顿**(快速滚动)→ 缓解:并发闸(2–3)+ 挂载触发的懒生成 + expo-image `allowDownscaling`。
- **需要重新原生构建** → 新增两个原生模块;不能在纯 Expo Go 跑,两端(arm64)需 dev-client / 构建产物重建。→ 缓解:并入正常 prebuild/build;在 tasks 里点明。
- **nativewind 静默失效**(expo-image 的 `className`)→ 缓解:`cssInterop` 注册或 `style` prop(D2)。
- **回收时闪旧图** → 缓解:`recyclingKey={item.id}` + `transition={0}`(D2)。
- **生成的视频 jpg 是全分辨率** → `getThumbnailAsync` 没有 `maxWidth/maxHeight`(只有 `quality`);jpg 是全帧。→ 接受:expo-image 在显示时降采样;将来切到 `generateThumbnailsAsync`(有 maxWidth/maxHeight)可缩小存储文件。

## Migration Plan

1. `npx expo install expo-image expo-video-thumbnails` 加依赖;两端重建原生。
2. 落 `media-type.ts`;把三处现有消费者改指过来。
3. 给 `FileBrowserItem` 加 `localUri`;更新 `fromSelectedFiles` / `fromInboxFiles`;去掉 `previewUriFor`;移除 `inboxPreviewUri`。
4. 加 `useFileThumbnail` + 视频缩略图缓存模块;把 `FileCard` 迁到 expo-image + hook + 播放角标;注册 nativewind `cssInterop`。
5. 更新 e2e fixture 与相关适配器测试到 `localUri` 契约。
6. 真机(emulator)验证:发送 scope 图片 + 视频海报;收件箱多文件网格 图片 + 视频;HEIC / 大图 / 长滚动;图标回退。回滚 = revert;无持久化 schema 或协议改动,故无数据迁移。

## Open Questions

- 桌面后续 B(视频):+40–80 MB 的 ffmpeg-sidecar 包体能否接受,还是桌面视频缩略图直接砍掉(桌面只做图片缩略图)?排期桌面 change 时再定。
- 是否给 `video-thumbs/` 加一个轻量缓存淘汰/大小上限,还是依赖 OS 对 cache 目录的回收?(倾向:先依赖 OS 回收;若增长再复查。)
