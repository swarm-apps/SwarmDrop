## 1. 准备工作

- [x] 1.1 创建 `src-tauri/src/mobile.rs` 文件
- [x] 1.2 在 `src-tauri/gen/android/app/build.gradle.kts` 添加 Kotlin 协程依赖 `kotlinx-coroutines-android:1.7.3`

## 2. Kotlin 插件实现

- [x] 2.1 创建 `src-tauri/gen/android/app/src/main/java/com/gy/swarmdrop/UpdaterPlugin.kt`
- [x] 2.2 定义 `@InvokeArg` 参数类 `InstallUpdateArgs`（包含 url 和 isForce 字段）
- [x] 2.3 实现 `@TauriPlugin` 类 `UpdaterPlugin`，继承 `Plugin(activity)`
- [x] 2.4 实现 `@Command` 方法 `installUpdate(invoke: Invoke)`
- [x] 2.5 在 `installUpdate` 中解析参数并调用 `MainActivity.startApkUpdate()`
- [x] 2.6 使用 `CoroutineScope(Dispatchers.Main)` 执行异步逻辑
- [x] 2.7 成功时调用 `invoke.resolve()`，失败时调用 `invoke.reject()`

## 3. Rust 桥接层

- [x] 3.1 在 `mobile.rs` 中定义 `PLUGIN_IDENTIFIER = "com.gy.swarmdrop"`
- [x] 3.2 实现 `init_updater()` 函数，调用 `api.register_android_plugin()`
- [x] 3.3 创建 `UpdaterPlugin<R>` 结构体包装 `PluginHandle`
- [x] 3.4 （可选）实现 `UpdaterPlugin::install_update()` 方法用于 Rust 调用

## 4. 应用集成

- [x] 4.1 在 `src-tauri/src/lib.rs` 顶部添加 `#[cfg(mobile)] mod mobile;`
- [x] 4.2 在 `run()` 函数的 `.setup()` 闭包中添加 Android 插件注册逻辑
- [x] 4.3 使用 `#[cfg(target_os = "android")]` 条件编译，仅在 Android 平台注册
- [x] 4.4 调用 `mobile::init_updater()` 并通过 `app.manage()` 管理插件状态
- [x] 4.5 添加错误处理和日志记录（使用 `tracing::error!`）

## 5. 前端适配（Android 后台下载）

- [x] 5.1 修改 `src/stores/upgrade-link-store.ts` 中的 `executeUpdate()` 方法
- [x] 5.2 Android 端调用改为 `invoke("plugin:updater|install_update", {url, isForce})`
- [x] 5.3 Android 端调用成功后不设置 `status: "downloading"`（避免显示进度条）
- [x] 5.4 Android 端可选：调用成功后关闭更新弹窗，或显示 Toast "后台下载中，请查看通知栏"
- [x] 5.5 保持桌面端原有逻辑：设置 `downloading` 状态，显示应用内进度条
- [x] 5.6 确保现有的事件监听器正确处理 `apk-download-done/error/cancel` 事件
- [x] 5.7 添加错误处理，捕获插件调用失败的情况

## 6. 清理废弃代码

- [x] 6.1 从 `src-tauri/src/lib.rs` 的 `invoke_handler!` 中移除 `commands::upgrade::install_android_update`
- [x] 6.2 删除 `src-tauri/src/commands/upgrade.rs` 中的 `install_android_update` 函数（或注释标记为 deprecated）

## 7. 测试验证

- [x] 7.1 运行 `pnpm android:dev` 确保 Android 项目编译成功
- [ ] 7.2 检查应用启动日志，确认插件注册成功（无错误）
- [ ] 7.3 在 Android 真机/模拟器上触发更新检查
- [ ] 7.4 点击"立即更新"，验证前端不显示进度条（或关闭弹窗）
- [ ] 7.5 验证系统通知栏显示下载进度（AppUpdater 通知）
- [ ] 7.6 验证下载时可以继续使用应用（非阻塞）
- [ ] 7.7 验证下载完成后触发 APK 安装流程
- [ ] 7.8 测试强制更新模式（`isForce=true`）
- [ ] 7.9 测试错误处理：网络断开、权限缺失等场景
- [ ] 7.10 验证桌面端仍显示应用内进度条（未受影响）
