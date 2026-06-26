## Why

SwarmDrop 目前使用 GitHub Releases 作为更新源，但缺乏灰度发布、强制升级等高级能力。集成 UpgradeLink 升级服务，可获得企业级更新管理能力，支持按设备/版本/百分比进行精准分发，降低新版本风险。

## What Changes

**桌面端（混合方案）**：
- TypeScript SDK 获取 UpgradeLink 升级策略（版本、升级类型、提示文案）
- 根据策略类型决定 UI 行为（强制/提示/静默）
- 调用 Tauri 官方 updater 执行实际下载安装

**移动端（混合方案）**：
- TypeScript SDK 获取 UpgradeLink 升级策略
- Rust Command 桥接到 Android Kotlin
- AppUpdater 库执行 APK 下载和安装

**CI/CD**：
- 添加 UpgradeLink 访问密钥配置（AccessKey / SecretKey）
- 修改 GitHub Actions 工作流，构建完成后同步版本信息至 UpgradeLink
- 桌面端和 Android 端分别同步到对应应用

## Capabilities

### New Capabilities
- `upgradelink-desktop-updater`: 桌面端混合更新方案（SDK 获取策略 + Tauri 执行更新）
  - Windows: `WinUpgradeRequest` + Tauri updater
  - macOS: `MacUpgradeRequest` + Tauri updater
  - Linux: `LnxUpgradeRequest` + Tauri updater
- `upgradelink-android-updater`: Android 端混合更新方案（SDK 获取策略 + AppUpdater 执行安装）
- `upgradelink-ci-sync`: CI/CD 流程中自动同步多平台版本信息至 UpgradeLink

### Modified Capabilities
- 无现有 spec 需要修改（更新机制属于实现层变更，不涉及用户-facing 的需求变更）

## Impact

- **前端**: 添加 `@toolsetlink/upgradelink-api-typescript` 依赖
- **桌面端**: 更新 `src-tauri/tauri.conf.json` updater 配置
- **Android**: 添加 `AppUpdate` 库依赖，新增 Rust→Kotlin 桥接代码
- **CI/CD**: `.github/workflows/release.yml` 添加 UpgradeLink 同步步骤
- **密钥管理**: 配置 `UPGRADE_LINK_ACCESS_KEY`, `UPGRADE_LINK_ACCESS_SECRET`, `UPGRADE_LINK_TAURI_KEY`, `UPGRADE_LINK_APK_KEY`
