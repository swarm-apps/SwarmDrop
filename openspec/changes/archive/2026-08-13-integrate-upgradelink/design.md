## Context

SwarmDrop 当前使用 GitHub Releases 作为 Tauri 桌面端更新源。虽然可以正常工作，但缺乏以下能力：
- 灰度发布（按百分比/设备分批推送）
- 强制升级（旧版本必须更新）
- 实时回滚（一键回退到旧版本）
- 升级数据看板

通过 UpgradeLink TypeScript SDK + 混合方案，可以统一桌面端和移动端的更新逻辑，同时利用平台原生能力完成安装。

## Goals / Non-Goals

**Goals:**
- 统一桌面端和移动端更新检查逻辑（共用 TypeScript SDK）
- 桌面端：SDK 获取策略 → 自定义 UI → Tauri updater 执行安装
- Android：SDK 获取策略 → 自定义 UI → AppUpdater 执行安装
- 支持灰度发布、强制升级、提示升级、静默升级策略
- CI/CD 自动同步多平台版本

**Non-Goals:**
- iOS 端更新（Tauri iOS 支持尚不成熟）
- 自建升级服务器
- 增量更新（等待 UpgradeLink / Tauri 支持）
- 配置热更新（Phase 2 可选）

## Decisions

### 1. 统一使用 TypeScript SDK 获取策略
- **选择**: `@toolsetlink/upgradelink-api-typescript`
- **桌面端**: 使用平台特定类 `WinUpgradeRequest`/`MacUpgradeRequest`/`LnxUpgradeRequest`
- **移动端**: 使用 `ApkUpgradeRequest`
- **理由**: 
  - 桌面端和移动端逻辑统一
  - 完全控制升级策略解析和 UI 展示
  - 支持 force/prompt/silent 多种升级类型
- **替代方案**: Tauri 原生 updater 直接对接 UpgradeLink 端点（无法控制策略解析）

### 2. 桌面端执行层使用 Tauri Updater
- **选择**: 调用 `@tauri-apps/plugin-updater` 的 `check()` 和 `downloadAndInstall()`
- **理由**: 
  - Tauri 官方维护，稳定可靠
  - 自动处理下载、签名验证、安装、重启
- **注意**: 需要配置 updater endpoints 指向 GitHub Releases（实际文件下载地址）

### 3. Android 执行层使用 AppUpdater
- **选择**: `io.github.azhon:appupdate:4.3.6`
- **理由**: 
  - 成熟的开源 Android 更新库
  - 支持强制更新、后台下载、进度显示
  - 适配国内网络环境
- **桥接方式**: TypeScript/Rust → Tauri Bridge → Kotlin MainActivity (无需手写 JNI，Tauri 自动处理)

### 4. CI 同步使用官方 Action
- **选择**: `toolsetlink/upgradelink-action@3.0.2`
- **理由**: 官方维护，与 Tauri Action 无缝衔接

### 5. 版本号管理
- **选择**: 统一使用语义化版本（semver）
- **桌面端**: version 字段（如 "0.1.2"）
- **Android**: versionCode（数字，如 100012）+ versionName（字符串）

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         Frontend (TypeScript)                    │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │  UpgradeLink SDK                                            │ │
│  │  - getTauriUpgrade() → Desktop strategy                     │ │
│  │  - getApkUpgrade()   → Android strategy                     │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                           ↓ parse strategy                       │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │  Update Store (Zustand)                                     │ │
│  │  - upgradeType: 'force' | 'prompt' | 'silent' | null       │ │
│  │  - versionName, downloadUrl, promptContent                 │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                           ↓ show UI                              │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │  Update Dialog (shadcn/ui)                                  │ │
│  │  - ForceUpdateDialog (阻塞式，必须更新)                      │ │
│  │  - PromptUpdateDialog (可选更新，有跳过按钮)                 │ │
│  │  - SilentUpdate (静默，后台处理)                             │ │
│  └─────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
                              ↓ execute
        ┌─────────────────────┴─────────────────────┐
        ↓                                           ↓
┌───────────────┐                       ┌───────────────────────────┐
│ Tauri Command │                       │   Tauri Command (Android) │
│  check()      │                       │   installApk(url: string) │
│  download()   │                       └───────────────────────────┘
│  install()    │                                    ↓
└───────────────┐                       ┌───────────────────────────┐
                │                       │   Kotlin MainActivity     │
                │                       │   - AppUpdate Manager     │
                │                       │   - Download & Install APK│
                │                       └───────────────────────────┘
```

## Risks / Trade-offs

| 风险 | 缓解措施 |
|-----|---------|
| UpgradeLink 服务不可用 | SDK 调用失败时静默处理，下次启动重试 |
| 升级策略解析失败 | 添加 try-catch，降级到提示更新 |
| Android 安装权限问题 | 引导用户开启"允许安装未知来源应用" |
| 版本号不一致 | CI 统一生成 versionCode（基于 semver） |
| 国内网络问题 | UpgradeLink 服务器位于国内，CDN 加速 |

## Migration Plan

### Phase 1: 基础 SDK 集成
1. 添加 `@toolsetlink/upgradelink-api-typescript` 依赖
2. 封装 `checkUpdate()` 函数（桌面端和 Android 端）
3. 更新检测 UI（复用现有 AboutSection 更新按钮）

### Phase 2: 桌面端完整流程
1. 配置 `tauri.conf.json` updater
2. 实现策略解析 → UI 展示 → Tauri updater 调用
3. 测试强制/提示/静默三种场景

### Phase 3: Android 端完整流程
1. Kotlin 添加 AppUpdater 依赖
2. Rust 添加 `install_android_update` command
3. Tauri Bridge 实现（在 `MainActivity.kt` 添加 `startApkUpdate()` 方法供前端调用）
4. 测试 APK 下载安装

### Phase 4: CI/CD 集成
1. 配置 GitHub Secrets
2. 更新 release.yml 添加 UpgradeLink 同步
3. 测试完整发布流程

## Open Questions

- [ ] 是否需要支持配置热更新（远程修改配置不发版）？
- [ ] Android 首次安装时是否需要提示用户开启安装权限？
- [ ] 是否需要在升级前自动备份用户数据？
