# 项目初始化与配置

## 环境准备

```bash
# Rust 工具链
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# C++ 编译工具
brew install cmake ninja                                 # macOS
sudo apt-get install cmake ninja-build                   # Linux

# Android 交叉编译 targets + cargo-ndk
rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android
cargo install cargo-ndk

# iOS 交叉编译 targets
xcode-select --install
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
```

支持矩阵:

- **React Native** ≥ 0.75(Turbo Module 要求);推荐 0.83+
- **Node.js** ≥ 18
- **包管理器**:yarn 集成最好,pnpm 也可(注意 hoisting / patch 路径,见 [build-pitfalls.md](build-pitfalls.md))

## 从零创建项目

### Step 1 — 用 create-react-native-library 生成脚手架

```bash
npx create-react-native-library@latest my-rust-lib
```

交互选项:

```text
✔ Library type:  Turbo module                  ← 必选
✔ Languages:     C++ for Android & iOS         ← 必选
✔ Example app:   Vanilla                       ← 推荐
```

```bash
cd my-rust-lib && yarn
(cd example/ios && pod install)
yarn example start
# 按 i 跑 iOS、按 a 跑 Android,看到 "Result: 21" 表示脚手架工作
```

### Step 2 — 安装 ubrn

```bash
yarn add uniffi-bindgen-react-native
```

在 `package.json` 加 scripts(下面是上游官方推荐写法):

```json
{
  "scripts": {
    "ubrn:ios":      "ubrn build ios     --config ubrn.config.yaml --and-generate && (cd example/ios && pod install)",
    "ubrn:android":  "ubrn build android --config ubrn.config.yaml --and-generate",
    "ubrn:web":      "ubrn build web     --config ubrn.config.yaml",
    "ubrn:checkout": "ubrn checkout      --config ubrn.config.yaml",
    "ubrn:clean":    "rm -rfv cpp/ android/CMakeLists.txt android/src/main/java android/*.cpp ios/ src/Native* src/index.*ts* src/generated/"
  }
}
```

清掉 builder-bob 模板默认生成的 C++(将被 ubrn 接管):

```bash
yarn ubrn:clean
```

### Step 3 — 编写 `ubrn.config.yaml`

最小化:

```yaml
---
rust:
  directory: ./rust          # 本地 Rust 源码目录
  manifestPath: Cargo.toml   # 相对 directory 的 Cargo.toml 路径

bindings:
  cpp: cpp/generated
  ts: src/generated
```

或从远程仓库 checkout:

```yaml
---
rust:
  repo: https://github.com/example/my-rust-sdk.git
  branch: main                                    # 也支持 rev / ref
  manifestPath: crates/my-api/Cargo.toml
```

### Step 4 — 准备 Rust crate

`Cargo.toml` 关键:

```toml
[lib]
crate-type = ["staticlib", "cdylib"]              # staticlib for iOS, cdylib for Android dynamic

[dependencies]
uniffi = { version = "0.31", features = ["cli", "build", "tokio"] }  # 用 async 必开 tokio

[build-dependencies]
uniffi = { version = "0.31", features = ["build"] }
```

`build.rs`:

```rust
fn main() {
    uniffi::generate_scaffolding("./src/lib.udl").ok();   // 仅 UDL 模式需要
    // 纯 proc-macro 模式无需 build.rs(但保留更省心)
}
```

`src/lib.rs`:

```rust
uniffi::setup_scaffolding!();

#[uniffi::export]
pub fn greet(name: String) -> String {
    format!("Hello, {name}!")
}
```

### Step 5 — 编译

```bash
yarn ubrn:ios            # 完整构建 iOS
yarn ubrn:android        # 完整构建 Android
yarn ubrn:web            # 完整构建 WASM

# 加快开发循环
yarn ubrn:ios --sim-only                                  # 只编模拟器
yarn ubrn:android --targets aarch64-linux-android         # 只编一个 arch
yarn ubrn:android --profile dev                           # 跳过优化
```

### Step 6 — 在 App 中使用

```typescript
import { greet } from "my-rust-lib";
console.log(greet("World"));    // "Hello, World!"
```

修改 Rust 后:`yarn ubrn:android` → 重启 metro。`pod install` 只在添加新文件后才需要。

## `ubrn.config.yaml` 完整字段

来自官方文档 [reference/config-yaml.md](https://jhugman.github.io/uniffi-bindgen-react-native/reference/config-yaml.html)。下面把全部段都列出来,**未列出的字段使用默认值即可**。

### `rust`

```yaml
rust:
  repo: https://github.com/example/my-rust-sdk     # 或:
  directory: ./rust                                 # 二选一
  branch: main                                      # repo 模式下:也支持 rev / ref
  manifestPath: crates/my-api/Cargo.toml
```

### `bindings`

```yaml
bindings:
  cpp: cpp/generated                                # 默认 cpp/generated
  ts: src/generated                                 # 默认 ts/generated
  uniffiToml: ./uniffi.toml                         # 可选,自定义类型映射
```

### `android`

```yaml
android:
  directory: ./android
  cargoExtras: []                                   # 透传给 cargo build 的额外参数
  targets:
    - arm64-v8a                                     # = aarch64-linux-android
    - armeabi-v7a                                   # = armv7-linux-androideabi
    - x86                                           # = i686-linux-android (模拟器)
    - x86_64                                        # = x86_64-linux-android (模拟器)
  apiLevel: 21                                      # 最低 API,透传给 cargo ndk --platform
  jniLibs: src/main/jniLibs                         # .so 文件位置
  packageName: <derived from package.json>          # 一般自动推断
  codegenOutputDir: <derived from package.json>     # 同上
  useSharedLibrary: true                            # true=cdylib, false=staticlib
```

> **`useSharedLibrary: true` 时**,Rust `[lib]` 必须 `crate-type = ["cdylib"]`,且 release profile 不能 `strip`(`strip = "none"`),否则 codegen 会失败。

### `ios`

```yaml
ios:
  directory: ios
  cargoExtras: []
  targets:
    - aarch64-apple-ios                             # 真机
    - aarch64-apple-ios-sim                         # Apple Silicon 模拟器
    # - x86_64-apple-ios                            # Intel 模拟器(老 Mac)
  xcodebuildExtras: []
  frameworkName: build/MyFramework                  # xcframework 名
  codegenOutputDir: <derived from package.json>
```

### `web`

```yaml
web:
  manifestPath: rust_modules/wasm/Cargo.toml
  manifestPatchFile: null                           # 可选 TOML 补丁文件
  wasmCrateName: <derived from package.json>
  features: []                                      # 同时用于编译 target crate + 写入 wasm crate Cargo.toml
  defaultFeatures: true
  workspace: false                                  # target crate 是否在已有 workspace
  runtimeVersion: <derived from ubrn>               # uniffi-runtime-javascript 版本(精确锁定)
  cargoExtras: []
  target: web                                       # 透传给 wasm-bindgen
  wasmBindgenExtras: []
  entrypoint: src/index.web.ts                      # 或 package.json browser 字段
  tsBindings: <same as bindings/ts>
```

> WASM 单线程,如果 Rust 代码用 `tokio`,启用 `uniffi-rs` 的 `wasm-unstable-single-threaded` feature(详见 [async-and-callbacks.md](async-and-callbacks.md))。

### `turboModule`

```yaml
turboModule:
  cpp: cpp                                          # generate jsi turbo-module 输出位置
  ts: <derived from package.json>
  spec: <derived from package.json>
  entrypoint: <derived from package.json>           # 默认 src/index.tsx
```

### `noOverwrite`

```yaml
noOverwrite:
  - "*.podspec"
  - CMakeLists.txt
  - android/build.gradle      # 见 build-pitfalls.md
  - package.json              # 同上
```

**glob 列表**,匹配的文件不会被 `--and-generate` / `generate jsi turbo-module` / `generate wasm wasm-crate` 覆盖。**首次生成后修补 build 文件,再加进 `noOverwrite`**,这是 ubrn 在生态没成熟前的标准操作。

## `ubrn` CLI 速查

| 子命令 | 作用 |
|--------|------|
| `ubrn checkout [REPO]` | clone 远程 Rust crate 到 `rust_modules/` |
| `ubrn build ios` | cargo build → xcframework |
| `ubrn build android` | cargo build → `.so`(通过 cargo-ndk) |
| `ubrn build web` | cargo build → WASM(通过 wasm-bindgen / wasm-pack) |
| `ubrn generate jsi bindings` | 仅生成 TS + C++ 绑定(不编译 Rust) |
| `ubrn generate jsi turbo-module` | 生成 Turbo Module 注册代码 |
| `ubrn generate wasm wasm-crate` | 生成 WASM 包装 crate |

通用 flag:

| Flag | 作用 |
|------|------|
| `--config <FILE>` | 指定配置文件,默认 `ubrn.config.yaml` |
| `--and-generate` / `-g` | 编译后立即跑 generate(最常用) |
| `--release` / `-r` | Release 模式 |
| `--profile <NAME>` | 自定义 profile(覆盖 `--release`) |
| `--targets <LIST>` | 逗号分隔的目标列表,覆盖 yaml 中的 targets |
| `--sim-only` / `--no-sim` | iOS 专用,只编模拟器 / 只编真机 |
| `--no-cargo` | 跳过 cargo build,只跑后续(已有产物时用) |

### 别名(可选)

```bash
alias ubrn=$(yarn uniffi-bindgen-react-native --path)    # yarn
# 或在 pnpm workspace
alias ubrn="pnpm exec ubrn"
```

## 生成的文件结构

单 crate 项目:

```text
my-rust-lib/
├── ubrn.config.yaml
├── rust/                              # 本地 Rust 源码(或 rust_modules/ 走 checkout)
├── cpp/generated/                     # C++ JSI 绑定 (.h + .cpp)
├── src/generated/                     # TypeScript 声明
│   ├── MyModule.ts                    # 公开 API(import 入口)
│   └── MyModule-ffi.ts                # FFI 底层(不直接 import)
├── android/
│   ├── src/main/jniLibs/              # 编译后的 .so(4 个 arch 各一份)
│   └── CMakeLists.txt                 # 生成的 CMake 配置
├── ios/
│   └── build/*.xcframework            # iOS 通用 framework
└── example/                           # 示例 App
```

多 crate(Megazord)布局见 [multi-crate-and-publish.md](multi-crate-and-publish.md)。

## 相关
- [type-mappings.md](type-mappings.md) — 写 Rust API 时的类型选择
- [build-pitfalls.md](build-pitfalls.md) — 首次构建几乎必踩的几个坑
- [multi-crate-and-publish.md](multi-crate-and-publish.md) — 工作区与发布
