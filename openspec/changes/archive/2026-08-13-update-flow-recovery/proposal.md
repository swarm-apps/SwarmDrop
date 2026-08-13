# update-flow-recovery

## Why

2026-08-08，v0.12.2 → v0.12.3 的移动端更新把用户锁死了：APK 下载完成的那一刻用户熄了屏，
系统安装确认框始终没有弹出，UI 停在「系统弹窗确认中…」；「点击安装」按钮是灰的点不动，
关掉弹窗后接管的进度框没有任何按钮、返回键也关不掉。唯一的出路是杀进程 —— 而杀进程会
丢掉全部已下载字节，重开只能从头再下。

根因有三处在上游 SwarmHive（engine 在调 adapter 前就销毁了句柄、`ready` 被当成过渡态、
registry 组件把 ready 态按钮禁用），已由该仓的 `ready-state-durability` change 处理。
本 change 负责本仓这一侧的三件上游管不到的事，外加一件三端体验的收口：

1. **弹窗里的 release notes 滚不动**（移动端）。`@rn-primitives/dialog` 的 `Content` 无条件
   挂 `onStartShouldSetResponder={() => true}`（本意是挡住点击穿透到遮罩），Android 上 JS
   responder 一旦授予就会 `blockNativeResponder`，把子层原生 `ScrollView` 的滚动禁掉。
   `ReleaseNotesView` 本身写得没问题（`maxHeight` + `nestedScrollEnabled`），是被祖先掐死的。
   同一份代码在 `alert-dialog` / `popover` / `select` / `tooltip` 里都有 —— **所有弹层内的
   ScrollView 都受影响**，不止更新弹窗。

2. **设置页把 `ready` 显示成「已是最新」**（两端同构）。移动端 `about.tsx` 的
   `hasUpdate = status === "available" || "force-required"` 是个二元判据，`ready` 落进 else
   分支；桌面 `-about-section.tsx` 的 `UpdateButton` 把 `downloading` 与 `ready` 并到同一个
   **disabled** 的「下载中...」按钮里。于是出现了截图里那一幕：背景写着「✅ 已是最新」，
   前面压着一个说有新版本要装的弹窗。

3. **下载在后台完成时用户无从知晓**。这正是本次故障的触发场景。`src/core/notifier.ts`
   已有高优先级通知渠道与「只检查不请求」的权限探测，接线即可，零新依赖。

4. **三端的更新呈现各写各的**。桌面 `UpdateButton` 是 switch、移动 `about.tsx` 是三元套三元、
   两边又都和 registry 组件重复了一遍状态判断。同一件事的第三、第四份实现，且都漏了 `ready`。

## What Changes

- 拉取上游两个 registry 的新版本（`ready` 态可点、进度弹窗可关、文案改口）
- 修 `mobile/src/components/ui/dialog.tsx` 与 `alert-dialog.tsx` 的 responder 抢占，
  让弹层内的 `ScrollView` 恢复可滚
- 两端设置页的更新状态判据补齐 `ready`，改为**穷尽 8 态**而不是二元/三元推导
- Android 下载完成时发一条本地通知，点击回到应用并落在可安装的位置
- 重写两份已经与实现脱节整整一个架构的 spec

## Capabilities

### New Capabilities

- `mobile-updater` — 移动端应用内更新的真实形态（RN + SwarmHive SDK + APK 侧载安装）

### Modified Capabilities

- `desktop-updater` — `ready` 态的呈现与出口；「下载完成即自动安装并重启」不再是无条件的
- `force-update` — 移动端强制更新的动作描述（不再是「跳浏览器下载 APK」）

### Removed Capabilities

- `mobile-version-check` — 描述的是 Tauri 移动端 fetch `latest.json` 的 `mobile.android`
  字段、再用 `tauri-plugin-opener` 跳浏览器下载 APK。**这三样今天一样都不存在**：移动端
  早已是 React Native + uniffi，更新走 SwarmHive endpoint 与应用内安装，`latest.json` 随
  UpgradeLink 一起被 SwarmHive 取代。整份 spec 描述的是一个不存在的实现，由
  `mobile-updater` 完整取代。

## Impact

**移动端（`mobile/`）**
- `src/components/ui/dialog.tsx`、`src/components/ui/alert-dialog.tsx` — 覆写
  `onStartShouldSetResponder`
- `src/app/settings/about.tsx` — 更新状态判据穷尽 8 态
- `src/lib/update-notification.ts`（新）— 下载完成通知，复用 `src/core/notifier.ts` 的渠道
- `src/components/update-host.tsx` — 接线通知与上游新 hook
- `src/lib/expo-downloader.ts`、`src/lib/expo-installer.ts`、`src/lib/rn-adapter.ts`、
  `src/lib/update-dialog-visibility.ts`、`src/lib/update-texts.ts`、
  `src/components/{prompt,force}-update-dialog.tsx`、`src/components/update-progress-dialog.tsx`、
  `src/components/update-settings-section.tsx` — **由 registry 重新拉取覆盖，不手改**

**桌面端（`src/`）**
- `src/routes/_app/settings/-about-section.tsx` — `UpdateButton` 拆开 `downloading` 与
  `ready`；`DownloadProgressBanner` 在 ready 态改口
- `src/lib/tauri-adapter.ts`、`src/lib/update-dialog-visibility.ts`、
  `src/components/*update*.tsx` — 由 registry-web 重新拉取覆盖

**依赖**
- `@swarm-hive/sdk` 升级到含 `reconcile` 端口的版本（本仓 `mobile/` 与根 `package.json` 两处）

**不改**：Web 端（`docs/app/app`）无应用内更新概念，刷新即最新。
