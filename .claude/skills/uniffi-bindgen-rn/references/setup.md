# uniffi-bindgen-rn: 项目搭建与配置

## 环境准备

```bash
# Rust 工具链
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# C++ 编译工具
brew install cmake ninja               # macOS
sudo apt-get install cmake ninja-build  # Linux

# Android targets + cargo-ndk
rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android
cargo install cargo-ndk

# iOS targets
xcode-select --install
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
```

## 从零创建项目

### Step 1: 用 builder-bob 生成脚手架

```bash
npx create-react-native-library@latest my-rust-lib
# 选择: Turbo module → C++ for Android & iOS → Vanilla
cd my-rust-lib && yarn
```

### Step 2: 添加 uniffi-bindgen-react-native

```bash
yarn add uniffi-bindgen-react-native
```

package.json scripts:
```json
{
  "ubrn:ios": "ubrn build ios --and-generate && (cd example/ios && pod install)",
  "ubrn:android": "ubrn build android --and-generate",
  "ubrn:web": "ubrn build web",
  "ubrn:checkout": "ubrn checkout",
  "ubrn:clean": "rm -rfv cpp/ android/CMakeLists.txt android/src/main/java android/*.cpp ios/ src/Native* src/index.*ts* src/generated/"
}
```

```bash
yarn ubrn:clean  # 清理 builder-bob 原始 C++ 模板
```

### Step 3: ubrn.config.yaml

```yaml
---
# 远程仓库
rust:
  repo: https://github.com/user/my-rust-crate.git
  branch: main
  manifestPath: crates/my-api/Cargo.toml

# 或本地目录（monorepo / submodule）
# rust:
#   directory: ./rust
#   manifestPath: Cargo.toml
```

完整配置:
```yaml
bindings:
  cpp: cpp/generated
  ts: src/generated
  uniffiToml: ./uniffi.toml

android:
  directory: ./android
  targets: [arm64-v8a, armeabi-v7a, x86, x86_64]
  apiLevel: 21
  jniLibs: src/main/jniLibs
  useSharedLibrary: true

ios:
  directory: ios
  targets: [aarch64-apple-ios, aarch64-apple-ios-sim]
  frameworkName: build/MyFramework

web:
  manifestPath: rust_modules/wasm/Cargo.toml

noOverwrite:
  - "*.podspec"
  - CMakeLists.txt
```

> `[lib]` 中必须声明 `crate-type = ["staticlib"]`(iOS) 或 `["cdylib"]`(Android)。

### Step 4-5: 拉取 Rust 代码 & 构建

```bash
yarn ubrn:checkout                              # 拉取到 rust_modules/
yarn ubrn:ios                                    # iOS 全量构建
yarn ubrn:android                                # Android 全量构建
yarn ubrn:ios --sim-only --and-generate          # 开发：仅模拟器
yarn ubrn:android --targets aarch64-linux-android # 开发：仅 arm64
```

## CLI 命令速查

| 命令 | 说明 |
|------|------|
| `ubrn checkout [REPO]` | 克隆 Rust 仓库到 `rust_modules/` |
| `ubrn build ios` | 编译 → xcframework |
| `ubrn build android` | 编译 → .so (cargo-ndk) |
| `ubrn build web` | 编译 → WASM (wasm-pack) |
| `ubrn generate jsi bindings` | 仅生成 TS + C++ 绑定 |

常用参数: `--and-generate`, `--release`, `--targets`, `--sim-only`, `--no-sim`, `--config`, `--profile`

## 生成的文件结构

```
my-rust-lib/
├── cpp/generated/          # C++ JSI 绑定 (.h + .cpp)
├── src/generated/          # TypeScript 声明 (.ts + -ffi.ts)
├── android/src/main/jniLibs/  # .so (4 架构)
├── ios/build/*.xcframework    # iOS 通用框架
└── rust_modules/              # Rust 源码
```
