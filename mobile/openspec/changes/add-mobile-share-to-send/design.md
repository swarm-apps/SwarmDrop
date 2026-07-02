## Context

现有发送流程是「设备优先」：主屏点在线设备 → `/send/select-device`（携带 `peerId`）→ 应用内
`DocumentPicker` 选文件 → `transfer-store.startSend({ files, peerId, peerName })`。文件必须是
`file://`（`DocumentPicker` 用 `copyToCacheDirectory` 把 Android `content://` 拷成 `file://`，
因为 Rust core 的 `ForeignFileAccess` 用 expo-file-system 打开源文件）。

本变更加入「文件优先」的入站入口：从别的 App 分享 → SwarmDrop → 选设备发送。这是与现有流程
**方向相反**的一条支路，需要系统级分享注册（iOS Share Extension / Android SEND intent）+ 一个
新的「选目标设备」屏。约束：core（`react-native-swarmdrop-core`，ubrn/uniffi 桥）不改；沿用
现有发送管线；两端都要真机可验证。

关键既有事实（探索确认）：
- `transfer-store.startSend({ files, peerId, peerName })` 直接吃文件参数，不依赖 `selectedFiles`。
- `ForeignFileAccess.readSourceChunk` 按 chunk 读 `file://` 源，已满足发送所需。
- `canSendToDevice` / `DeviceCard` / `file-tree` 摘要可直接复用。
- 原生构建走 `pnpm prebuild → pnpm --filter react-native-swarmdrop-core build:ios/android → app build`。

## Goals / Non-Goals

**Goals:**
- SwarmDrop 出现在系统分享面板（file/image/video），一步把分享的文件发给已配对在线设备。
- 复用现有发送管线，core 零改动。
- 覆盖冷/热启动、节点未启动、无在线设备、未过引导等前置态。

**Non-Goals:**
- 纯文本 / URL 分享（v1 不做；需 core 新增文本发送路径）。
- 「分享给多台设备」「离线排队等设备上线自动发」——本次只做单目标、即时发送。
- 大文件的存储优化（去掉双份拷贝）——先接受一份 App 内拷贝。
- 改动传输协议 / 收件箱 / 配对逻辑。

## Decisions

### D1. 用 `expo-share-intent`（而非手写或 `expo-share-extension` / `burnt`）
一个 config plugin 同时生成 iOS Share Extension（含 App Group 接线）与 Android
`ACTION_SEND`/`ACTION_SEND_MULTIPLE` intent-filter，并用 `useShareIntent` 把分享内容交付 JS，
**且把分享项拷成 `file://` 路径**（正好等于现有 `TransferFile.sourceId` 形态）。
- 备选：手写 Android intent-filter 插件 + iOS Share Extension（重复造轮子，iOS 部分尤其繁琐）；
  `expo-share-extension`（maxast，用于在分享面板内渲染自定义 React 界面，对「只接收+打开主 App」
  过重）；`burnt`（只做 toast，不相关）。→ `expo-share-intent` 覆盖面与产物形态最契合。

### D2. 分享文件一律拷成 App 拥有的 `file://`，不直接用 `content://`
Android `ACTION_SEND` 的 `content://` 读权限是**临时**的（绑接收 intent 的生命周期）、且
**不能 `takePersistableUriPermission`**，撑不过分钟级的大文件传输 / 退后台。所以必须拷成 App
自己拥有、权限稳定的 `file://`。`expo-share-intent` 已内建此拷贝（Android 拷进 cache、iOS 经
Share Extension 拷进 App Group 容器）。
- 备选：直接把 `content://` 当 `sourceId` 喂 core（`readSourceChunk` 读取侧看似支持 content
  URI）。→ 否决：权限生命周期不可靠，长传会中断。拷贝换稳定性是正确取舍。

### D3. 反向流用**专门的「选目标设备」屏** `/app/send/share-target.tsx`
分享是「文件已定、挑设备」，与 `select-device`（设备已定、挑文件）相反，不复用后者。新屏顶部
用 `file-tree` 摘要展示分享的文件、下方用 `DeviceCard` 列出 `canSendToDevice` 的在线已配对设备，
点一个 → `startSend({ files, peerId, peerName })` → `router.replace('/transfer/[sessionId]')`。
- 备选：记住上次设备直接发（误发风险）；退回主屏把设备行切 picker 态（心智绕、状态耦合）。
  → 专门屏最清晰、可控。

### D4. 直接调 `startSend` 传 files，不经 `selectedFiles` store
`startSend` 已接受 `{ files, peerId, peerName }`。分享流把映射好的 `TransferFile[]` 直接传入，
不污染供交互式发送页用的 `mobile-core-store.selectedFiles`。

### D5. 入站在根布局用 `ShareIntentProvider` / `useShareIntent`
在 `_layout.tsx` 包 `ShareIntentProvider`；一个 handler 拿到 `shareIntent.files` → 映射
`TransferFile[]`（`sourceId=path, name=fileName, relativePath=fileName, size`）→ 按状态路由：
- 未过引导 → toast「请先完成设置」+ 跳引导，**放弃本次分享**（v1 不做暂存/恢复，见下方已定决议）；
- 已过引导 → push `/send/share-target`；节点未启动交给该屏自动 `startNode`。
处理完调 `resetShareIntent()` 防重复。冷启动靠 `useShareIntent` 的初始值 + App ready 门控；
热启动靠其事件（Android `onNewIntent` / iOS 由库处理）。

### D6. 映射与文件名
`shareIntent.files[].{ path, fileName, size }` → `TransferFile{ sourceId: path, name: fileName,
relativePath: fileName, size: BigInt(size ?? 0) }`。分享无目录结构，`relativePath` 用平铺文件名。

## Risks / Trade-offs

- **iOS Share Extension 内存/时限杀大文件** → 拷贝走流式（内存可控，主要是 I/O），但超大文件
  （GB 级）仍可能超时被杀；v1 记为已知限制，必要时加分享体积上限提示；主体传输仍在主 App 完成。
- **双份存储（分享的 App 内拷贝）** → 传输结束 / cleanup 时删除 App 拥有的分享临时文件，避免堆积。
- **config plugin 与现有自定义 plugin 冲突** → prebuild 后人工核对 `AndroidManifest`（SEND filter
  与既有 `swarmdrop://` VIEW filter、`singleTask` 共存）与 iOS 工程（Share Extension target +
  entitlement）合并结果。
- **必须原生重编** → 走项目标准链（prebuild 会重生成 native 工程，故须重建原生桥 `build:ios/android`；
  core 不改）。属常规成本，非阻塞。
- **iOS App Group 需 provisioning** → 在 Apple 账号注册 `group.com.yexiyue.swarmdrop`，一次性。
- **冷启动竞态**（分享早于 providers/引导就绪）→ 门控在 App ready + 引导完成后再路由；未就绪先暂存。

## Migration Plan

纯新增、无数据迁移。上线 = 加依赖/插件 → prebuild → 重建原生桥 → app build → 商店更新。
回滚 = 移除 `expo-share-intent` 插件与新屏/接线 → prebuild + 重建 → 分享入口消失，其余不受影响。

## Open Questions

- ~~未过引导的处理~~ **已定（shape 确认）**：v1 简化——未引导时 toast「请先完成设置」+ 跳引导，
  放弃本次分享，不做暂存/恢复状态机。
- **iOS 大文件体积上限**：是否设阈值提示，阈值取多少？（实现后定）
- ~~App 内分享拷贝的清理时机~~ **已定（实现中发现）**：**不能**在「发送成功」后删——`startSend`
  返回时传输才刚启动，core 后续按 chunk **还在读**这份 `file://` 拷贝，删了会毁掉进行中的传输。
  v1 不主动删,靠 OS cache 清理;要主动清理须挂到「传输完成」事件(按 sessionId 删源文件),留后续。
