# 多 Crate(Megazord)与发布

## 多 Crate Cargo 工作区

实际项目通常拆 crate(`core` / `network` / `db` / `api`)。uniffi 支持把多个 crate 打成一个动态库,Mozilla 把这种打包叫 **Megazord**。

```text
my-lib/
├── Cargo.toml                  # [workspace]
├── crates/
│   ├── core/                   # 业务核心
│   │   ├── Cargo.toml
│   │   └── src/lib.rs          # #[uniffi::export]
│   ├── network/                # 网络模块
│   │   ├── Cargo.toml
│   │   └── src/lib.rs          # #[uniffi::export]
│   └── api/                    # 统一入口 crate
│       ├── Cargo.toml          # 依赖 core + network
│       └── src/lib.rs          # uniffi::setup_scaffolding!() + pub use
```

`ubrn.config.yaml` 指向**入口 crate** 的 `Cargo.toml`:

```yaml
rust:
  directory: ./rust
  manifestPath: crates/api/Cargo.toml
```

入口 crate `lib.rs`:

```rust
uniffi::setup_scaffolding!();

// re-export(可选,只是为了让 Rust 端 use 起来方便)
pub use core::*;
pub use network::*;
```

> 入口 crate 必须包含 `uniffi::setup_scaffolding!()`,这是 uniffi-rs 的 scaffolding 入口。**每个产生 cdylib 的 crate 只能调一次**。

### ubrn 怎么处理多 crate?

ubrn 会**为每个含 uniffi export 的 crate 生成一对 TS + C++**。例如 `core` 和 `network` 各自的 export 会变成:

```text
cpp/generated/
├── core.cpp
├── core.hpp
├── network.cpp
└── network.hpp
src/generated/
├── core.ts
├── core-ffi.ts
├── network.ts
└── network-ffi.ts
src/index.tsx                   # ubrn 自动生成,re-export 两个 crate
```

TS 端 import:

```typescript
// 顶层 named import(推荐)
import { CoreType } from "my-megazord";
import { NetworkType } from "my-megazord";

// 或 namespace 风格
import megazord from "my-megazord";
const { CoreType } = megazord.core;
const { NetworkType } = megazord.network;
```

### ⚠️ 跨 crate 类型重名是大坑

Swift 的 module 系统限制——**同一个 Megazord 里不允许重名类型**。两个 crate 各有一个 `Error` enum 会撞,编译报 `Detected duplicate identifier`。

修法只有一种:**真的把 Rust 名字改掉**(`CoreError` vs `NetworkError`)。`#[serde(rename)]` 不影响 uniffi 类型名。

### 入口 crate 的纯 wrap 模式

实践中常见的设计:核心业务 crate 完全平台无关(也不 derive uniffi 注解),**只在入口 crate 写 wrap 层**——用 uniffi 注解暴露 FFI 表面,然后调用核心 crate:

```text
shared/                          # 跨平台核心(不依赖 uniffi)
├── crates/core/
└── crates/network/

my-rust-lib/                     # RN 库
├── ubrn.config.yaml
└── rust/wrap/                   # 入口 crate(依赖 shared/core, shared/network)
    └── src/
        ├── lib.rs               # uniffi::setup_scaffolding!()
        ├── error.rs             # FfiError + From<CoreError>
        ├── events.rs            # FfiEvent + From<CoreEvent>
        ├── types.rs             # FfiUser / FfiDoc 投影(具有 specta-friendly 形状)
        └── api.rs               # #[uniffi::export] impl 调 core
```

好处:

- 核心 crate 可同时被 Tauri 桌面 / RN / WASM / CLI 复用
- FFI 边界类型可以根据各 host 调优(RN 用 `String` 替代 `PeerId`,WASM 简化时间戳)
- 上游 core 升级不需要 fork uniffi 注解

### wrap 层不要只代理 core,要核对桌面端 command 的副作用

如果你的桌面端(Tauri command)在 core 调用之外**还**做了关键副作用——比如发布事件 / 触发 sync / 写另一张表——RN wrap **也必须做这些事**。否则编译通过、单端正常,但跨端协作 / 远端同步会**静默失效**。

```rust
// ❌ 只代理 core
pub async fn apply_update(&self, doc_id: String, data: Vec<u8>) -> Result<(), FfiError> {
    self.inner.apply_update(&doc_id, &data).await.map_err(FfiError::from)?;
    Ok(())
}

// ✅ 对照桌面 command 补足副作用
pub async fn apply_update(&self, doc_id: String, data: Vec<u8>) -> Result<(), FfiError> {
    self.inner.apply_update(&doc_id, &data).await.map_err(FfiError::from)?;
    if let Some(sync) = self.inner.sync().await {
        sync.publish_update(&doc_id, data).await;    // 桌面 command 也做了这一步
    }
    Ok(())
}
```

**结论**:每个 wrap 方法落地时,对照桌面 command 实现,把"core 调用之外的副作用"逐项补齐。

## `uniffi.toml` 配置(可选)

如果需要给某个外部类型自定义 TS 表示,或调整某些 bindings 行为:

```toml
[bindings.typescript]
module_name = "my-megazord"                      # TS module 名

[bindings.typescript.custom_types.Uuid]
type_name = "string"                              # TS 类型
into_custom = "({}).toString()"
try_from_custom = "({})"                          # 接收 string 视作 Uuid

# 关闭 Object 配套的 Interface 生成
[bindings.typescript.objects.MyObject]
no_interface = true
```

详见上游 [reference/uniffi-toml.md](https://jhugman.github.io/uniffi-bindgen-react-native/reference/uniffi-toml.html)。

## 发布方式

### 选项 1:预编译二进制(推荐)

npm 包含编译好的 `.so` / `.xcframework`,用户安装后**无需 Rust 工具链**。

```json
// package.json
{
  "files": [
    "android",          // jniLibs/*.so
    "build",            // *.xcframework
    "cpp",              // C++ JSI 绑定
    "src",              // TypeScript
    "ios",
    "*.podspec"
  ]
}
```

**⚠️ `.gitignore` 与 `files` 的交互**:

npm 默认参考 `.gitignore` 决定打包内容。常见的 `.gitignore` 会忽略 `*.a` 和 `build/`,导致预编译产物**被排除在包外**。

**修复**:在 `package.json` 的 `files` 数组里**显式包含**这些目录——`files` 优先级高于 `.gitignore`。

验证打包内容:

```bash
npm pack --dry-run                              # 列出会被打进 tarball 的文件
```

确认 `build/*.xcframework` 与 `android/src/main/jniLibs/*/lib*.so` 都在。

### 选项 2:源码发布(包小但要求用户有 Rust 工具链)

```json
{
  "scripts": {
    "postinstall": "yarn ubrn:checkout && yarn ubrn:android --release && yarn ubrn:ios --release"
  }
}
```

适合内部库 / 团队工具。开源库 → 80% 用户没有 Rust 环境,**强烈不推荐**。

### 选项 3:混合(开发用源码,发布用预编译)

```json
{
  "scripts": {
    "prepublishOnly": "yarn ubrn:android --release && yarn ubrn:ios --release"
  },
  "files": ["android/jniLibs", "build", "src", "cpp", "ios", "*.podspec"]
}
```

git 仓库不存二进制,但 `npm publish` 时编一份打进去。

## 发布前检查清单

- [ ] 全架构 release 编译通过:
  - iOS: `aarch64-apple-ios`(真机)+ `aarch64-apple-ios-sim`(模拟器)
  - Android: `arm64-v8a` + `armeabi-v7a` + `x86` + `x86_64`(共 4 个 .so)
- [ ] `npm pack --dry-run` 检查产物存在
- [ ] 真机 + 模拟器都跑过 example app
- [ ] TS 类型导出正确(`import { ... } from "your-lib"` 有补全)
- [ ] `package.json` 含正确的 `main` / `types` / `react-native` 字段
- [ ] README 标注 `uniffi-bindgen-react-native` 出处(参考社区做法)

## CI 自动化

GitHub Actions 模板:

```yaml
name: Release
on:
  push:
    tags: ['v*']

jobs:
  build-ios:
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: aarch64-apple-ios,aarch64-apple-ios-sim
      - run: yarn install
      - run: yarn ubrn:ios --release
      - uses: actions/upload-artifact@v4
        with: { name: ios-build, path: build/ }

  build-android:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: aarch64-linux-android,armv7-linux-androideabi,i686-linux-android,x86_64-linux-android
      - run: cargo install cargo-ndk
      - run: yarn install
      - run: yarn ubrn:android --release
      - uses: actions/upload-artifact@v4
        with: { name: android-jni, path: android/src/main/jniLibs/ }

  publish:
    needs: [build-ios, build-android]
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/download-artifact@v4
      - run: yarn install
      - run: npm publish --access public
        env: { NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }} }
```

iOS build 必须在 macOS runner(`xcrun lipo` / `xcodebuild` 依赖);Android 在 Linux runner 即可。

## 相关
- [setup.md](setup.md) — `ubrn.config.yaml` 完整字段
- [build-pitfalls.md](build-pitfalls.md) — 跨平台编译常见坑
- [type-mappings.md](type-mappings.md) — wrap 层投影 core 类型
