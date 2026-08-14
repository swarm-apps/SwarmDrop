## ADDED Requirements

### Requirement: Plugin registration
系统 SHALL 在 Android 应用启动时注册更新插件到 Tauri 运行时。

#### Scenario: 成功注册插件
- **WHEN** Android 应用启动并执行 setup 阶段
- **THEN** UpdaterPlugin 被注册到 Tauri 插件系统
- **AND** 插件可通过 `plugin:updater|*` 格式被前端调用

#### Scenario: 非 Android 平台跳过注册
- **WHEN** 应用在桌面端（Windows/macOS/Linux）启动
- **THEN** 跳过 Android 插件注册逻辑
- **AND** 应用正常启动，无错误日志

### Requirement: APK 下载和安装
系统 SHALL 提供通过 AppUpdater 下载并安装 APK 的能力。

#### Scenario: 正常更新流程
- **WHEN** 前端调用 `plugin:updater|install_update` 并传入 `{url, isForce}`
- **THEN** UpdaterPlugin 解析参数并调用 MainActivity.startApkUpdate()
- **AND** AppUpdater 在系统通知栏显示下载进度
- **AND** 下载完成后自动触发安装

#### Scenario: 强制更新
- **WHEN** 前端调用插件时 `isForce=true`
- **THEN** AppUpdater 配置为强制更新模式（`isForcedUpgrade=true`）
- **AND** 用户无法取消下载

#### Scenario: 下载失败处理
- **WHEN** APK 下载过程中发生错误（网络断开、存储空间不足等）
- **THEN** UpdaterPlugin 通过 `invoke.reject()` 返回错误信息
- **AND** 前端收到错误并更新 UI 状态

### Requirement: 安装权限检查
系统 SHALL 在下载前检查是否具有安装未知来源应用的权限。

#### Scenario: 已授予权限
- **WHEN** Android 8.0+ 设备已授予安装权限
- **THEN** 直接开始 APK 下载

#### Scenario: 未授予权限
- **WHEN** Android 8.0+ 设备未授予安装权限
- **THEN** 引导用户到系统设置页面授予权限
- **AND** 用户授权后通过 Tauri 事件 `apk-install-permission-granted` 通知前端

### Requirement: 前端事件通知
系统 SHALL 通过 Tauri 事件系统向前端发送下载状态更新。

#### Scenario: 下载完成通知
- **WHEN** APK 下载完成
- **THEN** 发送 `apk-download-done` 事件到前端
- **AND** 前端更新 UI 为"正在安装"状态

#### Scenario: 下载取消通知
- **WHEN** 用户在通知栏取消下载
- **THEN** 发送 `apk-download-cancel` 事件到前端
- **AND** 前端重置 UI 为初始状态

#### Scenario: 下载错误通知
- **WHEN** 下载过程中发生错误
- **THEN** 发送 `apk-download-error` 事件，携带错误信息 `{error: string}`
- **AND** 前端显示错误提示

### Requirement: 参数验证
系统 SHALL 验证前端传入的参数有效性。

#### Scenario: 有效参数
- **WHEN** 前端传入 `{url: "https://example.com/app.apk", isForce: true}`
- **THEN** UpdaterPlugin 成功解析参数
- **AND** 开始下载流程

#### Scenario: URL 缺失
- **WHEN** 前端传入的参数中 `url` 为空或缺失
- **THEN** UpdaterPlugin 通过 `invoke.reject()` 返回参数错误
- **AND** 不启动下载流程

### Requirement: 协程异步处理
系统 SHALL 使用 Kotlin 协程在主线程上异步执行更新逻辑，避免阻塞 UI。

#### Scenario: 异步执行
- **WHEN** UpdaterPlugin 接收到更新请求
- **THEN** 在 `CoroutineScope(Dispatchers.Main)` 中启动协程
- **AND** UI 线程不被阻塞，应用保持响应

### Requirement: 后台下载体验
系统 SHALL 在 Android 端使用系统通知栏显示下载进度，前端不显示应用内进度条。

#### Scenario: Android 端调用插件后立即返回
- **WHEN** 前端调用 `plugin:updater|install_update`
- **THEN** UpdaterPlugin 立即通过 `invoke.resolve()` 返回成功
- **AND** AppUpdater 在后台下载，通知栏显示进度
- **AND** 前端可以关闭更新对话框或重置状态
- **AND** 用户可以继续使用应用（非阻塞式下载）

#### Scenario: 桌面端显示应用内进度
- **WHEN** 桌面端执行更新
- **THEN** 前端显示进度对话框
- **AND** 实时更新下载百分比和速度
- **AND** 阻塞式下载（等待完成后重启）

#### Scenario: Android 端不设置 downloading 状态
- **WHEN** Android 端调用插件成功返回
- **THEN** 前端不设置 `status: "downloading"`
- **AND** 前端不显示进度条组件
- **AND** 可选：显示 Toast 提示"正在后台下载，请查看通知栏"
