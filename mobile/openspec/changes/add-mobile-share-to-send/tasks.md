## 1. 依赖与配置

- [x] 1.1 加依赖 `expo-share-intent`（pnpm，7.0.0）
- [x] 1.2 `app.json` 加 `expo-share-intent` 插件：`iosActivationRules`（File/Image/Movie，不含 text/url）、`iosAppGroupIdentifier`（`group.com.yexiyue.swarmdrop`）、`androidIntentFilters` / `androidMultiIntentFilters`（`*/*`，覆盖文件/图片/视频；文本/URL 由 handler 在应用内拒绝）
- [ ] 1.3 核对与既有自定义 plugin（`with-android-install-permission` / `with-android-release-signing`）叠加无冲突 —— **需 prebuild 后核对合并结果（你的构建步）**

## 2. 原生构建打通（你来）

- [ ] 2.1 `pnpm prebuild`，核对 `AndroidManifest` 新增 `ACTION_SEND`/`ACTION_SEND_MULTIPLE` intent-filter 且与既有 `swarmdrop://` VIEW filter + `singleTask` 共存
- [ ] 2.2 核对 iOS 工程生成 Share Extension target + App Group entitlement
- [ ] 2.3 Apple 账号注册 App Group `group.com.yexiyue.swarmdrop` + provisioning（iOS 一次性）
- [ ] 2.4 重建原生桥：`pnpm --filter react-native-swarmdrop-core build:android` 与 `build:ios`
- [ ] 2.5 两端 app build + 安装成功

## 3. 入站接线（根布局）

- [x] 3.1 `_layout.tsx` 包 `ShareIntentProvider`（`options={{ debug: __DEV__ }}`），无常驻 UI
- [x] 3.2 `src/core/share-intent.ts`：`shareIntent.files` → `TransferFile[]`（`sourceId=file:// path`，平铺文件名，多文件，跳过无 path 项）
- [x] 3.3 `ShareIntentHandler`：已引导 → push `/send/share-target`；未引导 → toast + 放弃；无文件（文本/URL 分享）→ toast + 放弃；处理后 `resetShareIntent()`
- [x] 3.4 冷启动 gated on `isReady` + App ready（`!ready` 早返回内不挂 handler）；热启动 push 叠栈，不打断进行中的传输
- [x] 3.5 `src/app/+native-intent.tsx`：`redirectSystemPath` 拦截 `dataUrl=<getShareExtensionKey()>` 分享 URL，重定向到 `/`（否则 expo-router 把 `swarmdrop://dataUrl=…?nonce=…` 当路由解析 → **Unmatched Route 404**）；分享数据由原生模块 keyed 保存，交给 `ShareIntentHandler`。**iOS 模拟器 E2E 实测发现的必需接线**

## 4. 选目标设备屏 `/app/send/share-target.tsx`

- [x] 4.1 顶部 `Surface`：`N 个文件 · 总大小` + 紧凑文件行（图标 + 名 + 大小 + 发前删单个）
- [x] 4.2 设备列表：`mergePairedDevicesWithCache` + 在线 + `canSendToDevice`，`TargetDeviceRow` 单选高亮
- [x] 4.3 点行选中 + 底部「发送给 X」→ `startSend({ files, peerId, peerName })` → `router.replace('/transfer/[sessionId]')`（含 `takeLastPanic` 错误详情、准备进度 `PrepareProgress`）
- [x] 4.4 节点未启动：进屏 `startNode` + 「正在启动节点…」态，起来后刷新在线设备
- [x] 4.5 无在线可发送设备：`EmptyState`（去配对/让对端上线）；离屏清空 share-store
- [x] 4.6 分享文件是 App cache 拷贝；**传输期间 core 仍在读，故不在发送后删（否则毁传输）**。双份存储靠 OS cache 清理；主动清理需挂「传输完成」事件，留后续（见 design 已定决议）

## 5. 前置态与文案

- [x] 5.1 多文件（`ACTION_SEND_MULTIPLE`）：映射与选设备屏均按数组处理
- [x] 5.2 新文案走 lingui（`Trans` / `t`）；`pnpm i18n:extract` 已跑（en 侧待翻译，非阻塞）

## 6. 验证

- [ ] 6.1 Android 真机（同 WiFi 物理机对传）：分享 文件/图片/视频 → 选设备屏 → 发送 → `/transfer` 闭环（你来）
- [ ] 6.2 iOS 真机（配好 App Group 后）：同上；重点大文件（Share Extension 内存/时限）与冷启动（你来）
- [ ] 6.3 边界：未引导 → 提示；节点未启动 → 自动启动；无在线设备 → 空态（你来，真机）
- [x] 6.4 `pnpm typecheck` + `biome check src` 通过；知识库记录 share-intent 接线与 App Group 要点
- [x] 6.5 iOS 模拟器 E2E（Maestro）：照片 → 分享 → SwarmDrop → 选设备屏 → 选 Mac-mini → 发送 → `/transfer` 建会话「等待响应」闭环通过；期间修掉 `+native-intent.tsx` 缺失导致的 Unmatched Route（真机大文件/App Group provisioning 仍待 6.2）
