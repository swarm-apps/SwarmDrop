## Context

SwarmDrop 当前使用 UpgradeLink 实现跨平台自动更新：
- **桌面端**：Tauri 官方 updater + UpgradeLink API 获取策略（混合方案）
- **Android 端**：前端 TypeScript SDK 调用 UpgradeLink API，但执行更新时的 Rust 命令 `install_android_update` 是空实现

问题根源：Tauri 2 中 Rust 无法直接调用 Android Activity 方法，需要通过插件机制桥接。现有的 `MainActivity.startApkUpdate()` 方法已实现 AppUpdater 下载逻辑，但无法被触发。

参考实现：
- 官方插件：tauri-apps/plugins-workspace/biometric
- 社区插件：Choochmeque/tauri-plugin-biometry

## Goals / Non-Goals

**Goals:**
- 修复 Rust → Kotlin 调用断层，使 Android 更新流程完整可用
- 采用 Tauri 2 官方推荐的插件机制（`@TauriPlugin` + `@Command`）
- 保持代码简洁，快速迭代（Phase 2 开发阶段）
- 不影响桌面端现有更新流程

**Non-Goals:**
- 创建独立的插件仓库或发布到 crates.io（业务特定功能，不做通用化）
- 修改 UpgradeLink SDK 集成方式（前端 TypeScript SDK 调用保持不变）
- 实现 Rust 到 Kotlin 的双向通信（只需单向：前端 → Kotlin）

## Decisions

### 1. 内联插件 vs 独立插件仓库

**决策**：采用内联插件方案（项目内部 `mobile.rs` + `UpdaterPlugin.kt`）

**理由**：
- 代码量小（2 个文件），维护成本低
- UpgradeLink 集成是业务特定逻辑，不具备复用价值
- 快速迭代：改完立即测试，无需管理 git 依赖
- 未来可低成本重构为独立插件（如需开源）

**备选方案**：`tauri plugin init` 创建独立仓库
- 优点：符合插件生态，可复用
- 缺点：过度设计，增加开发复杂度

### 2. 插件注册方式

**决策**：在 `lib.rs` 的 `setup()` 中手动注册插件

```rust
.setup(|app| {
    #[cfg(target_os = "android")]
    {
        let api = PluginApi::new(app.handle());
        let updater = mobile::init_updater(app.handle(), api)?;
        app.manage(updater);
    }
    Ok(())
})
```

**理由**：
- 遵循官方插件实现模式（参考 biometry 插件）
- `setup()` 是注册移动端插件的标准时机
- 可以优雅处理错误（如移动端不支持时跳过）

**备选方案**：通过 `.plugin()` 链式调用
- 优点：更简洁
- 缺点：需要创建完整的 `TauriPlugin` 结构，复杂度高

### 3. 前端调用方式

**决策**：前端直接调用插件 `invoke("plugin:updater|install_update", {...})`

**理由**：
- Tauri 插件系统自动路由到 Kotlin `@Command` 方法
- 避免 Rust 层无意义的中转代码
- 调用链路清晰：TypeScript → Tauri IPC → Kotlin

**备选方案**：通过 Rust 命令中转
- 优点：前端 API 保持不变
- 缺点：Rust 层需要调用 `run_mobile_plugin()`，增加复杂度

### 4. 包名选择

**决策**：使用应用包名 `com.gy.swarmdrop` 作为 `PLUGIN_IDENTIFIER`

**理由**：
- 与 MainActivity 同包，便于访问
- 避免包名冲突
- 简化 Kotlin 代码结构

### 5. Android 下载进度显示策略

**决策**：Android 端使用系统通知栏显示进度，桌面端使用应用内进度条

**理由**：
- AppUpdater 自带系统通知栏进度显示，这是 Android 用户熟悉的原生体验
- 避免前端重复显示进度（通知栏 + 应用内）造成混乱
- Android 用户习惯在通知栏查看下载进度（系统下载、应用市场更新等）
- 允许用户在下载时继续使用应用（后台下载，非阻塞式）
- 简化前端实现：Android 端无需维护下载进度状态

**实现细节**：
- **Android 端**：
  - 调用 `plugin:updater|install_update` 后立即返回成功
  - 前端不设置 `status: "downloading"`（不显示进度条）
  - 可选：关闭更新弹窗或显示 Toast "正在后台下载，请查看通知栏"
  - AppUpdater 在系统通知栏显示进度（已配置 `isShowNotification=true`）
  - 通过事件监听 `apk-download-done/error/cancel` 更新 UI
- **桌面端**：
  - 保持应用内进度条显示
  - 阻塞式下载，实时更新百分比和速度
  - 下载完成后自动重启

**备选方案**：Android 端也显示应用内进度
- 优点：跨平台 UI 一致性
- 缺点：
  - 需要前端从 Kotlin 回传进度数据（增加复杂度）
  - 与系统通知栏进度重复
  - 用户无法在下载时使用应用（需要停留在更新页面）

## Risks / Trade-offs

| 风险 | 影响 | 缓解措施 |
|------|------|---------|
| **插件注册失败** | Android 端更新功能不可用 | 添加详细日志，setup 中捕获错误但不阻塞应用启动 |
| **协程依赖缺失** | `CoroutineScope` 编译错误 | 在 `build.gradle.kts` 中添加 `kotlinx-coroutines-android` 依赖 |
| **包名不匹配** | `register_android_plugin` 找不到类 | 确保 PLUGIN_IDENTIFIER、Kotlin 包名、UpdaterPlugin 类名一致 |
| **前端调用格式错误** | 插件命令无法触发 | 严格遵循格式 `plugin:<插件名>|<命令名>`，添加错误处理 |

**Trade-offs**:
- ✅ **简单性 > 复用性**：内联插件不可复用，但大幅降低开发成本
- ✅ **快速迭代 > 完美架构**：先让功能跑起来，后续可重构
- ❌ **前端 API 变更**：从 `invoke("install_android_update")` 改为 `invoke("plugin:updater|install_update")`（可接受的小改动）

## Migration Plan

### 部署步骤
1. 创建 `mobile.rs` 和 `UpdaterPlugin.kt`
2. 修改 `lib.rs` 注册插件
3. 添加 Kotlin 协程依赖
4. 修改前端调用代码
5. 移除废弃的 Rust 命令
6. 在 Android 真机测试完整更新流程

### 回滚策略
如果插件注册失败或调用出错：
- 前端可回退到直接调用 Rust 命令（保留旧代码注释）
- Rust 可临时实现 JNI 调用（复杂度高，仅作最后手段）

### 验证清单
- [ ] Android 编译通过
- [ ] 应用启动无错误日志
- [ ] 前端能触发更新检查
- [ ] 点击"立即更新"后 AppUpdater 开始下载
- [ ] 下载完成后触发安装

## Open Questions

- ~~是否需要在 iOS 端实现类似插件？~~ → 暂不需要，iOS 使用 App Store 更新机制
- ~~`UpdaterPlugin` 是否需要实现其他命令（如取消下载）？~~ → 暂不需要，AppUpdater 已提供通知栏取消功能
