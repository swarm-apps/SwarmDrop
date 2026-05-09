# 桌面端自动更新

## 概述

SwarmDrop 桌面端（Windows / macOS / Linux）使用 `tauri-plugin-updater` 实现静默检查 + 用户确认下载安装的自动更新。移动端（Android）由于系统限制，改为引导用户下载 APK。

整个更新系统围绕一个 `latest.json` 清单文件运作，该文件随每次 Release 发布到 GitHub。

## 架构总览

```
┌─────────────────────────────────────────────────────────┐
│                  GitHub Release                          │
│                                                          │
│  latest.json ─── 版本信息 + 平台下载链接 + 签名          │
│  *.msi.zip   ─── Windows 安装包 + .sig 签名文件         │
│  *.dmg       ─── macOS 安装包 + .sig 签名文件           │
│  *.AppImage  ─── Linux 安装包 + .sig 签名文件           │
│  *.apk       ─── Android APK（无签名验证）              │
└──────────────────┬──────────────────────────────────────┘
                   │
        ┌──────────┴──────────┐
        │                     │
   桌面端客户端          移动端客户端
        │                     │
   tauri-plugin-        fetch latest.json
   updater 自动检查     解析 mobile.android
        │                     │
   校验 minisign       比较版本号
   签名 → 下载 →       → 跳转浏览器
   安装 → 重启            下载 APK
```

## 依赖配置

### Rust 端

`src-tauri/Cargo.toml`：

```toml
tauri-plugin-updater = "2"
tauri-plugin-process = "2"    # 重启用
```

### 前端

```bash
pnpm add @tauri-apps/plugin-updater @tauri-apps/plugin-process
```

### 插件注册

`src-tauri/src/lib.rs` 中，updater 在 `setup` 回调中注册（移动端不支持时容错跳过）：

```rust
.plugin(tauri_plugin_process::init())
.setup(|app| {
    // updater 在 setup 中注册，移动端不支持时容错跳过
    if let Err(e) = app
        .handle()
        .plugin(tauri_plugin_updater::Builder::new().build())
    {
        tracing::warn!("Failed to initialize updater plugin: {e}");
    }
    // ...
    Ok(())
})
```

### 权限

`src-tauri/capabilities/default.json`：

```json
{
  "permissions": [
    "updater:default",
    "process:allow-restart"
  ]
}
```

### Tauri 配置

`src-tauri/tauri.conf.json`：

```json
{
  "bundle": {
    "createUpdaterArtifacts": true
  },
  "plugins": {
    "updater": {
      "pubkey": "<minisign 公钥 base64>",
      "endpoints": [
        "https://github.com/swarm-apps/SwarmDrop/releases/latest/download/latest.json"
      ]
    }
  }
}
```

- `createUpdaterArtifacts: true` — 构建时自动生成 `.sig` 签名文件
- `pubkey` — minisign 公钥，用于客户端验证更新包完整性
- `endpoints` — `latest.json` 的下载地址

### 签名密钥

使用 Tauri 内置的 minisign 签名体系：

```bash
cargo tauri signer generate -w keys/swarmdrop.key
```

生成文件：
- `keys/swarmdrop.key` — 私钥（CI 使用，通过 `TAURI_SIGNING_PRIVATE_KEY` Secret 传入）
- `keys/swarmdrop.key.pub` — 公钥（写入 `tauri.conf.json` 的 `plugins.updater.pubkey`）

## latest.json 结构

标准 Tauri updater 格式 + 自定义移动端扩展字段：

```json
{
  "version": "0.2.0",
  "notes": "Bug fixes and performance improvements",
  "pub_date": "2025-02-14T12:00:00Z",
  "platforms": {
    "windows-x86_64": {
      "url": "https://github.com/.../swarmdrop_0.2.0_x64-setup.msi.zip",
      "signature": "<minisign 签名>"
    },
    "darwin-aarch64": {
      "url": "https://github.com/.../swarmdrop.app.tar.gz",
      "signature": "<minisign 签名>"
    },
    "linux-x86_64": {
      "url": "https://github.com/.../swarmdrop_0.2.0_amd64.AppImage",
      "signature": "<minisign 签名>"
    }
  },
  "min_version": "0.1.0",
  "mobile": {
    "android": {
      "version": "0.2.0",
      "download_url": "https://github.com/.../swarmdrop_0.2.0_aarch64.apk",
      "min_version": "0.1.0"
    }
  }
}
```

| 字段 | 说明 |
|------|------|
| `platforms` | 标准 Tauri 字段，由 `tauri-apps/tauri-action` 自动生成 |
| `min_version` | 自定义扩展，低于此版本的客户端将被强制更新 |
| `mobile.android` | 自定义扩展，由 CI 的 `update-latest-json` job 注入 |

## 前端实现

### 文件结构

```
src/
├── commands/updater.ts          # 桌面端 updater API 封装
├── lib/version.ts               # semver 比较 + latest.json 类型定义
├── stores/update-store.ts       # 更新状态机（Zustand）
├── components/
│   ├── ForceUpdateDialog.tsx     # 强制更新弹窗
│   └── settings/AboutSection.tsx # 设置页更新 UI
```

### 更新状态机

`update-store.ts` 定义了 8 个状态：

```
idle → checking → up-to-date
                → available → downloading → ready（自动重启）
                → force-required → downloading → ready
                → error
```

关键 actions：

| Action | 说明 |
|--------|------|
| `checkForUpdate()` | 自动分流：桌面端调用 tauri-plugin-updater，移动端 fetch latest.json |
| `downloadAndInstall()` | 桌面端下载 + 安装 + 重启 |
| `openDownloadPage()` | 移动端跳转浏览器下载 APK |

### 桌面端更新流程

```typescript
// src/commands/updater.ts
import { check } from "@tauri-apps/plugin-updater";

const update = await check();       // 自动从 endpoints 下载 latest.json 并比对版本
if (update) {
  await update.downloadAndInstall(  // 下载 + 验证 minisign 签名 + 安装
    (event) => { /* 进度回调 */ }
  );
  await relaunch();                 // 重启应用
}
```

### 移动端更新流程

```typescript
// update-store.ts → checkMobileUpdate()
const res = await fetch(LATEST_JSON_ENDPOINTS[0]);
const json: LatestJson = await res.json();
const android = json.mobile?.android;

if (isVersionLessThan(currentVersion, android.version)) {
  // 有新版本 → 设置 status = "available"
  // 用户点击 → openUrl(android.download_url)
}
```

### 强制更新逻辑

当 `latest.json` 包含 `min_version` 字段时：

```typescript
if (isVersionLessThan(currentVersion, minVersion)) {
  set({ status: "force-required" });
  // ForceUpdateDialog 弹出，不可关闭
}
```

`ForceUpdateDialog` 是一个全屏模态弹窗，阻止用户继续使用应用：
- 桌面端：显示「立即更新」按钮，原地下载安装
- 移动端：显示「前往下载」按钮，跳转浏览器

### 启动自动检查

应用启动后延迟 3 秒自动检查：

```typescript
// 在 App 入口
setTimeout(() => {
  useUpdateStore.getState().checkForUpdate();
}, 3000);
```

## UI 展示

### 设置页 AboutSection

位于设置页底部，展示应用信息和更新状态：

- **idle / error**：显示「检查更新」按钮
- **checking**：按钮变为加载状态「检查中...」
- **up-to-date**：副标题显示「已是最新版本」
- **available**：蓝色 banner 展示版本信息 + 更新日志，桌面端「更新到 vX.X.X」/ 移动端「前往下载」
- **downloading**：进度条 + 百分比 + 已下载/总大小 + 速度

### 强制更新弹窗 ForceUpdateDialog

全屏不可关闭弹窗，包含：
- 红色警告图标 + 版本信息卡片（当前版本 / 最低要求 / 最新版本）
- 黄色警告条：「P2P 协议已变更，旧版本无法与其他设备通信」
- 下载进度状态切换

## 版本比较

`src/lib/version.ts` 提供 semver 比较工具：

```typescript
compareVersions("1.2.3", "1.3.0")  // → -1
isVersionLessThan("0.1.0", "0.2.0") // → true
```

特性：
- 忽略 `v` 前缀（`v1.2.3` → `1.2.3`）
- 忽略预发布标签的内容，但有预发布标签的版本 < 无标签版本（`1.0.0-beta` < `1.0.0`）

## GitHub Secrets

| Secret | 说明 |
|--------|------|
| `TAURI_SIGNING_PRIVATE_KEY` | minisign 私钥内容（`keys/swarmdrop.key` 文件内容） |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | 私钥密码 |

这些密钥用于 CI 构建时对更新包进行签名，客户端使用 `tauri.conf.json` 中的公钥验证。
