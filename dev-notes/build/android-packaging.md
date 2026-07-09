# Android 打包配置指南

## 概述

SwarmDrop 使用 Tauri v2 的 Android 支持构建 APK。由于项目依赖 `libsodium`（通过 `swarm-p2p-core` → `libsodium-sys-stable`），Android 交叉编译需要预构建的 libsodium 静态库。

## 目录结构

```
src-tauri/
├── gen/android/                          # Tauri 生成的 Android 项目
│   ├── app/build.gradle.kts              # 应用级构建配置（签名、SDK 版本）
│   ├── keystore.properties               # 本地签名配置（git-ignored）
│   └── ...
├── vendor/
│   └── prebuilt/android-aarch64/         # 预构建的 libsodium 静态库
│       ├── libsodium.a
│       └── liblibsodium.a
keys/
└── swarmdrop.jks                         # Android 签名密钥库（git-ignored）
scripts/
└── android.mjs                           # Android 构建辅助脚本
```

## 环境要求

| 工具 | 版本 | 说明 |
|------|------|------|
| Java (JDK) | 17 | `setup-java@v4` 使用 Temurin 发行版 |
| Android SDK | 最新 | 通过 Android Studio 或 `setup-android@v3` |
| Android NDK | 随 SDK 安装 | CI 上使用 `$ANDROID_NDK_LATEST_HOME` |
| Rust target | `aarch64-linux-android` | `rustup target add aarch64-linux-android` |
| Node.js | LTS | pnpm 11 |

## 本地开发

### 1. 安装 Rust Android 目标

```bash
rustup target add aarch64-linux-android
```

### 2. 确保 NDK 已安装

通过 Android Studio → SDK Manager → SDK Tools → NDK (Side by side) 安装。

### 3. 开发模式

```bash
pnpm android:dev
```

此命令由 `scripts/android.mjs` 处理，自动设置 `SODIUM_LIB_DIR` 环境变量指向预构建库：

```javascript
// scripts/android.mjs
const libDir = resolve("src-tauri/vendor/prebuilt/android-aarch64");
process.env.SODIUM_LIB_DIR = libDir;
execSync(`tauri android ${command}`, { stdio: "inherit", env: process.env });
```

### 4. 构建 APK

```bash
pnpm android:build
```

等价于 `SODIUM_LIB_DIR=... tauri android build --apk`。

## 签名配置

### 密钥库生成

项目已在 `keys/swarmdrop.jks` 生成了签名密钥库：

```bash
keytool -genkey -v \
  -keystore keys/swarmdrop.jks \
  -keyalg RSA -keysize 2048 \
  -validity 10000 \
  -alias <ALIAS> \
  -storepass <PASSWORD> \
  -keypass <PASSWORD> \
  -dname "CN=SwarmDrop, OU=Dev, O=SwarmDrop, L=Unknown, ST=Unknown, C=CN"
```

### 本地签名配置

`src-tauri/gen/android/keystore.properties`（已在 `.gitignore` 中排除）：

```properties
keyAlias=<ALIAS>
keyPassword=<PASSWORD>
storePassword=<PASSWORD>
storeFile=../../../keys/swarmdrop.jks
```

### Gradle 签名集成

`build.gradle.kts` 中的签名配置读取 `keystore.properties`：

```kotlin
signingConfigs {
    create("release") {
        val keystorePropertiesFile = rootProject.file("keystore.properties")
        val keystoreProperties = Properties()
        if (keystorePropertiesFile.exists()) {
            keystoreProperties.load(FileInputStream(keystorePropertiesFile))
        }
        keyAlias = keystoreProperties["keyAlias"] as String
        keyPassword = keystoreProperties["keyPassword"] as String
        storeFile = file(keystoreProperties["storeFile"] as String)
        storePassword = keystoreProperties["storePassword"] as String
    }
}

buildTypes {
    getByName("release") {
        signingConfig = signingConfigs.getByName("release")
        isMinifyEnabled = true
        // ...
    }
}
```

### CI 环境签名

在 GitHub Actions 中，签名信息通过 Secrets 注入：

| Secret | 说明 |
|--------|------|
| `ANDROID_KEY_ALIAS` | 密钥别名 |
| `ANDROID_KEY_PASSWORD` | 密钥密码（同时用于 keyPassword 和 storePassword） |
| `ANDROID_KEY_BASE64` | `.jks` 文件的 base64 编码 |

生成 base64 编码：

```bash
base64 -i keys/swarmdrop.jks | tr -d '\n'
```

CI 中动态创建 `keystore.properties`：

```yaml
- name: Setup Android signing
  run: |
    cd src-tauri/gen/android
    echo "keyAlias=${{ secrets.ANDROID_KEY_ALIAS }}" > keystore.properties
    echo "keyPassword=${{ secrets.ANDROID_KEY_PASSWORD }}" >> keystore.properties
    echo "storePassword=${{ secrets.ANDROID_KEY_PASSWORD }}" >> keystore.properties
    base64 -d <<< "${{ secrets.ANDROID_KEY_BASE64 }}" > $RUNNER_TEMP/keystore.jks
    echo "storeFile=$RUNNER_TEMP/keystore.jks" >> keystore.properties
```

## libsodium 交叉编译

### 为什么需要预构建库？

`swarm-p2p-core` 依赖 `libsodium-sys-stable`，该库在 Android 交叉编译时无法自动编译。因此项目在 `src-tauri/vendor/prebuilt/android-aarch64/` 提供了预构建的静态库。

### 环境变量

| 变量 | 用途 |
|------|------|
| `SODIUM_LIB_DIR` | 指向预构建 libsodium 的目录 |
| `NDK_HOME` / `ANDROID_NDK_HOME` / `ANDROID_NDK_ROOT` | 指向 Android NDK 安装路径 |

### CI 上的替代方案

在 Linux CI 环境中，也可以通过系统包安装 libsodium-dev：

```bash
sudo apt-get install -y libsodium-dev
```

但 Android 交叉编译仍需要 `SODIUM_LIB_DIR` 指向 ARM64 架构的静态库，系统安装的是 x86_64 架构，仅用于主机端编译工具。

## Android SDK 版本

在 `build.gradle.kts` 中配置：

```kotlin
android {
    compileSdk = 36
    defaultConfig {
        minSdk = 24      // Android 7.0
        targetSdk = 36   // Android 15
    }
}
```

## 安全注意事项

以下文件不应提交到版本控制：

- `keys/` — 签名密钥库和 minisign 密钥
- `src-tauri/gen/android/keystore.properties` — 签名凭据
- `.jks` 文件 — 密钥库二进制

已通过 `.gitignore` 排除。
