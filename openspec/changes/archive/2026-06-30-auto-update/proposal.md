## Why

SwarmDrop 是去中心化文件传输工具，P2P 协议版本兼容性直接决定设备间能否通信。当协议发生 breaking change 时，旧版客户端将无法与新版设备建立连接。因此需要内置自动更新机制，确保用户及时获取新版本，并在协议不兼容时强制更新。

## What Changes

- 桌面端（Windows/macOS/Linux）集成 `tauri-plugin-updater`，支持静默下载 + 签名验证 + 自动安装重启
- 移动端（Android）通过 fetch `latest.json` 检测版本，引导用户跳转浏览器下载 APK
- 新增 `latest.json` 版本清单，托管在 GitHub Releases，包含桌面端签名 URL 和移动端 APK 下载链接
- 新增 `min_version` 字段，当本地版本低于最低要求时触发强制更新弹窗（不可关闭）
- 设置页「关于」区域新增版本检测与更新状态展示（无更新 / 有更新 / 下载中）
- CI/CD 新增多平台构建签名 + `latest.json` 组装 job
- **BREAKING**: 强制更新时用户必须更新才能继续使用应用

## Capabilities

### New Capabilities
- `desktop-updater`: 桌面端自动更新 — tauri-plugin-updater 集成、签名验证、下载进度、自动安装重启
- `mobile-version-check`: 移动端版本检测 — fetch latest.json、版本比较、跳转浏览器下载
- `force-update`: 强制更新机制 — min_version 检查、不可关闭弹窗、阻断应用使用
- `update-ui`: 更新 UI 组件 — 设置页关于区域状态展示、强制更新弹窗、下载进度条

### Modified Capabilities
<!-- 无现有 spec 需要修改 -->

## Impact

- **Rust 依赖**: 新增 `tauri-plugin-updater`、`tauri-plugin-process`
- **前端依赖**: 新增 `@tauri-apps/plugin-updater`、`@tauri-apps/plugin-process`
- **Tauri 配置**: `tauri.conf.json` 新增 updater plugin 配置（pubkey、endpoints）、`createUpdaterArtifacts: true`
- **Capabilities**: `default.json` 新增 `updater:default`、`process:allow-restart` 权限
- **CI/CD**: GitHub Actions 新增签名密钥管理、多平台构建产物签名、latest.json 组装
- **状态管理**: 可能新增 update store 或扩展现有 store
- **路由守卫**: 强制更新需要在应用入口阻断导航
