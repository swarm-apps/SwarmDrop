## REMOVED Requirements

### Requirement: 移动端版本检测

**Reason**: 描述的实现已整体不存在。该 spec 写的是「Tauri 移动端启动时 HTTP fetch
`latest.json`，解析 `mobile.android` 字段」；今天移动端是 React Native + Expo + uniffi，
更新检查走 `@swarm-hive/sdk` 的 `checkUpdateAndroid` 打 SwarmHive endpoint，
`latest.json` 这个载体本身已随 UpgradeLink → SwarmHive 的迁移消失。

**Migration**: 由 `mobile-updater` 的「移动端应用内更新通道」取代。

### Requirement: 移动端引导下载

**Reason**: 「提供『前往下载』按钮，点击后通过 `tauri-plugin-opener` 打开浏览器跳转到 APK
下载链接」—— 三个要素全部作废：移动端没有 Tauri、没有 `tauri-plugin-opener`，更新也不再
跳浏览器。现在是应用内下载到 cache，再交系统 PackageInstaller 安装。

**Migration**: 由 `mobile-updater` 的应用内下载与安装流程取代。

### Requirement: latest.json 移动端扩展

**Reason**: `latest.json` 及其 `mobile.android.{version,download_url,min_version}` 字段随
UpgradeLink 一起被移除。移动端的 release 元数据现由 SwarmHive server 按 app slug
`swarmdrop-rn` 提供，wire 契约归 SwarmHive 仓的 `update-check-rn-android` capability。

**Migration**: 无需迁移 —— CI 早已不生成该文件。契约的事实源移交 SwarmHive 仓。
