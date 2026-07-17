## Why

目前把文件从别的 App 发出去，用户必须先打开 SwarmDrop、选好设备、再用应用内的
DocumentPicker 反过来挑文件——与移动端「在任意 App 里选中内容 → 分享 → 选目标 App」的
直觉相反，多了一次上下文切换。让 SwarmDrop 出现在系统分享面板里、一步把选中的文件发给
已配对设备，是消费级文件传输 App 的标配入口，也直接支撑 PRODUCT.md 的「毫不费力地完成
一次设备到设备的发送」。

## What Changes

- 让 SwarmDrop 注册为**系统分享目标**：iOS Share Extension + Android `ACTION_SEND` /
  `ACTION_SEND_MULTIPLE` intent（经 `expo-share-intent` 的 config plugin 生成）。
- 分享进来的文件由库拷贝成 App 拥有的 `file://`（权限稳定，可撑住长传），映射成现有的
  `TransferFile`，**复用现有 `prepareSend` / `sendPrepared` 发送管线，core 侧零改动**。
- 新增一个**反向流的「选目标设备」屏**（`/send/share-target`）：顶部展示分享的文件摘要，
  下方列出在线可发送的已配对设备，点一个即发，跳到 `/transfer/[sessionId]` 看进度。
- 根布局接入 `ShareIntentProvider` / `useShareIntent`，处理冷启动（App 被分享拉起）与热启动
  （App 已在前台）两种入站，并处理前置态：未过引导（暂存分享内容、引导后恢复）、节点未启动
  （自动启动）、无在线可发送设备（空状态）。
- v1 只接收 **文件 / 图片 / 视频**；纯文本 / URL 分享不在本次范围。
- iOS 需一次性配置 **App Group**（跨 Share Extension 与主 App 传递文件）。

不改动传输协议、Rust core、收件箱或已配对设备的既有行为——纯新增一个入站入口。

## Capabilities

### New Capabilities
- `mobile-share-to-send`: 把 SwarmDrop 注册为系统分享目标，接收其它 App 分享的文件/图片/
  视频，落到「选目标设备」屏，复用现有发送管线一步发给已配对的在线设备；覆盖入站解析、
  冷/热启动、前置态（未引导/节点未启动/无在线设备）与平台差异（iOS Share Extension + App
  Group / Android SEND intent）。

### Modified Capabilities
<!-- 无:openspec/specs/ 目前为空,没有既有能力的需求发生变化;发送/传输的既有行为不变。 -->

## Impact

- **新依赖**：`expo-share-intent`（config plugin + `useShareIntent` hook）。
- **配置**：`app.json` 加插件与 `iosActivationRules`（file/image/movie）、
  `androidIntentFilters`、`iosAppGroupIdentifier`；prebuild 后 `AndroidManifest` 新增 SEND
  intent-filter、iOS 生成 Share Extension target + App Group entitlement。与现有自定义
  plugin（`with-android-install-permission` / `with-android-release-signing`）叠加。
- **新代码**：`/app/send/share-target.tsx`（选设备屏）、根布局（`_layout.tsx`）的
  ShareIntentProvider 接线 + 冷/热启动路由 + 未引导暂存、分享文件 → `TransferFile` 映射工具。
- **复用**：`transfer-store.startSend({ files, peerId, peerName })`、`canSendToDevice`、
  `DeviceCard`、`file-tree` 摘要、`mobile-core-store` 的设备列表。
- **原生构建**：走项目标准链 `pnpm prebuild → pnpm --filter react-native-swarmdrop-core
  build:ios/android → app build`（prebuild 会重生成 native 工程，故须重建原生桥；core 本身
  不改）。iOS 另需在 Apple 账号注册 App Group。
- **已知限制**：iOS Share Extension 有内存/时限，超大文件（GB 级视频）拷入 App Group 容器
  存在被系统杀的风险；v1 记为已知限制。分享的文件会产生一份 App 内拷贝（双份存储）。
