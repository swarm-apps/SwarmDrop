# CI/CD 工作流

## 概述

SwarmDrop 使用 GitHub Actions 实现自动化发布。工作流文件位于 `.github/workflows/release.yml`，由 tag push 触发，自动构建全平台安装包并发布到 GitHub Release。

## 触发方式

```yaml
on:
  push:
    tags:
      - 'v*'           # 推送 v 开头的 tag 自动触发
  workflow_dispatch:
    inputs:
      min_version:      # 手动触发时可指定强制更新最低版本
        description: '强制更新最低版本号 (如 0.2.0)，留空则不设置'
```

发版流程：

```bash
# 1. 修改版本号
# src-tauri/tauri.conf.json → "version": "0.2.0"

# 2. 提交并打 tag
git add -A && git commit -m "release: v0.2.0"
git tag v0.2.0
git push && git push --tags

# 3. GitHub Actions 自动构建 + 发布 Draft Release
# 4. 在 GitHub 确认 Release 后发布
```

## 工作流全景

```
release.yml
│
├── build-tauri (并行 4 平台)
│   ├── macOS aarch64 (Apple Silicon)
│   ├── macOS x86_64 (Intel)
│   ├── Ubuntu 22.04 (Linux)
│   └── Windows latest
│
├── build-android (并行)
│   └── Ubuntu latest (交叉编译 aarch64)
│
└── update-latest-json (需等待上述两个 job 完成)
    └── 下载 latest.json → 注入 mobile.android 字段 → 重新上传
```

## Job 详解

### build-tauri — 桌面端多平台构建

使用 `tauri-apps/tauri-action@v0` 官方 Action，自动完成：
- 前端编译（`pnpm build`）
- Rust 交叉编译
- 安装包打包（.msi, .dmg, .AppImage 等）
- minisign 签名（使用 `TAURI_SIGNING_PRIVATE_KEY`）
- 生成 `latest.json`（标准 Tauri updater 格式）
- 上传到 GitHub Draft Release

**构建矩阵：**

| 平台 | Runner | 构建参数 | 产物 |
|------|--------|---------|------|
| macOS ARM | macos-latest | `--target aarch64-apple-darwin` | `.dmg` + `.app.tar.gz` |
| macOS Intel | macos-latest | `--target x86_64-apple-darwin` | `.dmg` + `.app.tar.gz` |
| Linux | ubuntu-22.04 | （默认） | `.AppImage` + `.deb` |
| Windows | windows-latest | （默认） | `.msi` + `.msi.zip` |

**平台特殊依赖：**

Ubuntu 需要安装系统库：

```bash
sudo apt-get install -y libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf
```

macOS 需要同时安装两个 Rust target：

```yaml
targets: aarch64-apple-darwin,x86_64-apple-darwin
```

**关键环境变量：**

| 变量 | 说明 |
|------|------|
| `GITHUB_TOKEN` | GitHub 自动提供，用于创建 Release |
| `TAURI_SIGNING_PRIVATE_KEY` | minisign 私钥，用于签名更新包 |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | 私钥密码 |

### build-android — Android APK 构建

在 Ubuntu runner 上交叉编译 ARM64 APK。

**步骤流程：**

1. **系统依赖** — `libsodium-dev`（主机端编译工具链需要）
2. **前端工具链** — pnpm 11 + Node.js LTS
3. **Java** — Temurin JDK 17
4. **Android SDK** — `android-actions/setup-android@v3`
5. **Rust** — stable + `aarch64-linux-android` target
6. **签名配置** — 从 Secrets 动态生成 `keystore.properties`
7. **构建** — `pnpm tauri android build --apk`
8. **上传** — 重命名为 `swarmdrop_{version}_aarch64.apk`，等待 Draft Release 创建后上传

**NDK 配置：**

不需要手动安装 NDK，GitHub runner 自带。通过 `$ANDROID_NDK_LATEST_HOME` 环境变量引用：

```yaml
env:
  NDK_HOME: ${{ env.ANDROID_NDK_LATEST_HOME }}
  ANDROID_NDK_HOME: ${{ env.ANDROID_NDK_LATEST_HOME }}
  ANDROID_NDK_ROOT: ${{ env.ANDROID_NDK_LATEST_HOME }}
  SODIUM_LIB_DIR: ${{ github.workspace }}/src-tauri/vendor/prebuilt/android-aarch64
```

**APK 上传等待逻辑：**

Android 构建与桌面端并行运行，但 Draft Release 由 `build-tauri` 的 `tauri-action` 创建。因此 Android job 需要轮询等待：

```bash
TAG="v${VERSION}"
for i in $(seq 1 30); do
  if gh release view "$TAG" &>/dev/null; then break; fi
  echo "Waiting for release $TAG... ($i/30)"
  sleep 10
done
gh release upload "$TAG" "$APK_NAME" --clobber
```

### update-latest-json — 补丁 latest.json

此 job 在两个构建 job 都完成后运行，作用是将移动端信息注入到 `latest.json`。

**流程：**

1. 从 Release 下载 `tauri-action` 生成的 `latest.json`
2. 用 `jq` 注入 `mobile.android` 字段（版本号 + APK 下载链接）
3. 如果手动触发时指定了 `min_version`，同时注入根级和 Android 级的 `min_version`
4. 重新上传覆盖

```bash
# 注入 mobile.android
jq --arg ver "$VERSION" --arg url "$APK_URL" \
  '.mobile.android = {version: $ver, download_url: $url}' \
  latest.json > tmp.json && mv tmp.json latest.json

# 注入 min_version（可选）
jq --arg mv "$MIN_VERSION" \
  '.min_version = $mv | .mobile.android.min_version = $mv' \
  latest.json > tmp.json && mv tmp.json latest.json
```

## 并发控制

```yaml
concurrency:
  group: release-${{ github.ref }}
  cancel-in-progress: true
```

同一 tag 的重复触发会取消前一次运行，避免重复发布。

## 缓存策略

### Rust 缓存

使用 `swatinem/rust-cache@v2`：

```yaml
- uses: swatinem/rust-cache@v2
  with:
    workspaces: './src-tauri -> target'
```

缓存 `src-tauri/target` 目录下的编译产物，显著加速后续构建。

### pnpm 缓存

通过 `actions/setup-node@v4` 的 `cache: 'pnpm'` 自动缓存 `node_modules`。

## GitHub Secrets 完整清单

| Secret | 说明 | 获取方式 |
|--------|------|---------|
| `TAURI_SIGNING_PRIVATE_KEY` | minisign 私钥 | `cat keys/swarmdrop.key` |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | 私钥密码 | 生成密钥时设置的密码 |
| `ANDROID_KEY_ALIAS` | Android 密钥别名 | 生成密钥时设置的别名 |
| `ANDROID_KEY_PASSWORD` | Android 密钥密码 | 生成密钥时设置的密码 |
| `ANDROID_KEY_BASE64` | .jks 文件 base64 | `base64 -i keys/swarmdrop.jks \| tr -d '\n'` |

## 发版检查清单

1. 更新 `src-tauri/tauri.conf.json` 中的 `version`
2. 确认 GitHub Secrets 已配置
3. 提交代码并推送 tag：`git tag v0.2.0 && git push --tags`
4. 等待所有 job 完成（约 15-25 分钟）
5. 在 GitHub Release 页面检查：
   - [ ] 桌面端安装包齐全（Windows .msi, macOS .dmg ×2, Linux .AppImage）
   - [ ] `.sig` 签名文件齐全
   - [ ] Android APK 已上传
   - [ ] `latest.json` 包含所有平台信息和 `mobile.android` 字段
6. 确认无误后将 Draft Release 发布为正式版

## 强制更新发版

当 P2P 协议发生不兼容变更时，需要强制更新：

1. 在 GitHub Actions 页面手动触发 workflow
2. 填入 `min_version`（如 `0.2.0`）
3. `latest.json` 将包含 `min_version` 字段
4. 低于此版本的客户端打开应用时会弹出不可关闭的强制更新弹窗
