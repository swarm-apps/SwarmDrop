## Why

当前 Android 端的 UpgradeLink 集成存在 Rust → Kotlin 调用断层：前端通过 TypeScript SDK 调用 UpgradeLink API 获取更新信息，但在执行更新时调用的 `install_android_update` Rust 命令是空实现，无法触发 Kotlin 层已实现的 `MainActivity.startApkUpdate()` 方法。这导致 Android 端无法完成 APK 下载和安装流程。

## What Changes

- 创建内联 Android 更新插件，使用 Tauri 2 官方插件机制桥接 Rust 和 Kotlin
- 新建 `mobile.rs` 模块注册 Kotlin 插件到 Tauri 运行时
- 新建 `UpdaterPlugin.kt` 作为 Tauri 插件实现，封装现有的 AppUpdater 逻辑
- 修改应用启动流程，在 `setup` 阶段注册 Android 插件
- 修改前端调用方式，从 `invoke("install_android_update")` 改为 `invoke("plugin:updater|install_update")`

## Capabilities

### New Capabilities
- `android-update-plugin`: Android APK 自动更新能力，提供通过 UpgradeLink + AppUpdater 实现的应用内更新功能

### Modified Capabilities
<!-- 无现有能力需要修改 -->

## Impact

**新增文件**：
- `src-tauri/src/mobile.rs` - Rust 插件注册层
- `src-tauri/gen/android/app/src/main/java/com/gy/swarmdrop/UpdaterPlugin.kt` - Kotlin 插件实现

**修改文件**：
- `src-tauri/src/lib.rs` - 添加 mobile 模块，在 setup 中注册插件
- `src-tauri/src/commands/upgrade.rs` - 移除空实现的 `install_android_update` 命令
- `src/stores/upgrade-link-store.ts` - 修改 Android 端调用方式

**依赖**：
- 需要添加 Kotlin 协程依赖到 `build.gradle.kts`
