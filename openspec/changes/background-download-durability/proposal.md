# background-download-durability

## Why

`update-flow-recovery` 解决了「下载**完成**后退出应用不必重下」（`reconcile` 恢复产物），
但用户原话里还有另一半没解决：

> 「或者下载了一半，我退出应用回来又得全部重新下载」

那一半在上一个 change 里被证明用 expo 的 API 做不到 —— `DownloadResumable.resumeData` 只有
`pauseAsync()` 会产出，进程被杀时没有钩子调得到它（推导见
`../SwarmHive/openspec/changes/ready-state-durability/design.md` 的 D7）。所以现在的行为是：
下载中断 = 全量重下，且已如实写进 spec 与知识库。

本 change 要把这一半真正做掉。

## 先别急着写代码：一个前提假设需要先被证伪

「加个前台服务就不会断了」这个直觉在本仓**可能是错的**，因为**前台服务已经在跑了**：
`mobile-core-store.ts:258` 在节点启动后就调 `startForegroundKeepAlive()`，节点停止时才拆
（`:204`）。也就是说，只要节点在运行，进程就已经有一张前台服务票挡着 LMK 回收。

那用户的下载为什么还是断了？至少三种可能，它们指向**完全不同**的修法：

| 假设 | 若成立，该怎么修 |
|---|---|
| A. 用户是从任务卡片上划**主动杀**掉应用 | 前台服务救不了（用户主动杀会停服务）。只有把下载交给**系统进程**才行 |
| B. 下载时节点没在运行，前台服务票不在 | 下载期间自己举一张票即可，改动很小 |
| C. 下载确实在跑，但 expo 的下载任务随 JS 引擎在后台被挂起 | 换下载实现，前台服务无济于事 |

**任务 1 就是把这三者分辨清楚**。在此之前不写任何实现代码 —— 上一个 change 已经因为
「看起来只差接线」而交付过一次无效实现，代价是一组绿色的、只证明了 mock 自己行为的测试。

## What Changes

方案空间按「能否解决假设 A」排序：

### 主推：把下载交给系统的 `DownloadManager`

Android 的 `DownloadManager` 是**系统服务**，下载由 system_server 执行：

- app 被杀（包括用户上划）下载照常继续 —— 直接解决假设 A
- **原生支持断点续传**（系统自己发 `Range`），不依赖任何 app 侧存档
- 自带系统下载通知，不与我们的前台服务通知争用那唯一一条

代价是 expo 没有封装它，需要一个原生模块或社区库（如
`@kesha-antonov/react-native-background-downloader`）。这是本 change 的主要决策点，
`design.md` 要评估：包体、维护活跃度、是否支持 `content://` 与我们已有的校验流程接口。

### 备选：下载期间自举前台服务票

只解决假设 B，改动最小（复用 `foreground-service.ts` 现成的能力）。但要处理一个硬约束：
**notifee 的前台服务通知是单例**（固定 `FGS_NOTIFICATION_ID`，idle 保活与传输进度已经在复用
同一条）。更新下载进度要么挤进那条通知，要么与传输进度互相覆盖 —— 需要明确的优先级规则。

### 不做

- **不自己实现 Range 续传**（手写 HTTP 下载器）：等于重新实现 `DownloadManager` 已经做好的事，
  且要自己处理重定向、镜像 fallback、断点校验。

## Capabilities

### Modified Capabilities

- `mobile-updater` — 「已下载**完成**的产物 SHALL 跨进程存活」这条要求将扩展到未完成的下载

## Impact

**取决于任务 1 的结论**，最坏情况（走 DownloadManager）：

- 新增原生依赖 + `app.json` 的 config plugin
- `mobile/src/lib/expo-downloader.ts` 换实现（registry 分发文件 —— 改动落在
  `../SwarmHive/packages/registry-rn/`，本仓只重新拉取）
- `ApkDownloader` 端口可能需要扩展「查询/接续一个进行中的下载」的语义
- `foreground-service.ts` 的通知优先级规则（若走备选方案）

**不改**：SDK 的 `reconcile` 端口与 `ready` 语义 —— 那一层已经对了，本 change 只换下载的执行者。
