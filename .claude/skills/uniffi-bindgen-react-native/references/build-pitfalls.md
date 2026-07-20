# 构建踩坑合集

ubrn 0.31.0-2 在生态融入度上还不算成熟,首次构建几乎必踩几个坑。本文按"出现概率"从高到低列出。

> **核心策略**:几乎所有"必踩坑"的修法都是改 ubrn 生成的 `build.gradle` / `package.json` / `podspec` / `CMakeLists.txt`,然后用 `ubrn.config.yaml` 的 **`noOverwrite`** 数组保护这些手改文件不被 `--and-generate` 覆盖。

```yaml
noOverwrite:
  - android/build.gradle
  - package.json
  - "*.podspec"
  - CMakeLists.txt
```

---

## 坑 1:async constructor 生成非法 TS `async static`

**症状**:`yarn prepare`(`bob build`)失败

```text
SyntaxError: src/generated/my_lib.ts: Unexpected token, expected "(" (NNNN:13)
  NNNN | async static create(...) /*throws*/ {
                ^
```

**原因**:ubrn 0.31.0-2 TS 模板 bug。Rust 端给 uniffi object 声明了 async primary constructor(`#[uniffi::constructor] pub async fn new(...)`),模板生成 `async ` + `static` + ` create(...)` → **`async static create(`**,TS 不接受这种修饰符顺序。

**所在文件**:`crates/ubrn_bindgen/src/bindings/gen_typescript/templates/ObjectTemplate.ts` + `macros.ts`(上游源码)。

**修复**:加个 post-process 脚本 `scripts/fix-ubrn-output.mjs`,扫 `src/generated/*.ts`,把 `async static` 换成 `static async`:

```javascript
import { readFileSync, writeFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

const dir = "src/generated";
for (const f of readdirSync(dir)) {
    if (!f.endsWith(".ts")) continue;
    const p = join(dir, f);
    const orig = readFileSync(p, "utf8");
    const fixed = orig.replace(/async\s+static\s+/g, "static async ");
    if (orig !== fixed) {
        writeFileSync(p, fixed);
        console.log(`fixed ${p}`);
    }
}
```

`package.json` 集成:

```json
{
  "scripts": {
    "ubrn:ios":     "ubrn build ios     --and-generate && pnpm ubrn:fix",
    "ubrn:android": "ubrn build android --and-generate && pnpm ubrn:fix",
    "ubrn:fix":     "node scripts/fix-ubrn-output.mjs"
  }
}
```

幂等(二次跑无 match 就不写盘),只改 `src/generated/` 下文件。

**什么时候可以删**:上游修了 `macros.ts` / `ObjectTemplate.ts` 之后(关注 release 里相关条目)。

---

## 坑 2:Windows `\\?\` 长路径前缀打穿 ld.lld

**症状**:`pnpm ubrn:android` 在 Windows 链接阶段大量

```text
ld.lld: error: cannot find version script \\?\D:\workspace\...\rustc6MrA9A\list
ld.lld: error: cannot open \\?\D:\workspace\...\deps\xxx.rcgu.o: No such file or directory
ld.lld: error: too many errors emitted, stopping now
```

**原因**:ubrn 用 `manifest_path.canonicalize_utf8()`(`camino` 包装 `std::fs::canonicalize`),**Windows 上无论路径多短都加 `\\?\` extended-length 前缀**。前缀传到 cargo → rustc → linker,而 `ld.lld` 不认识 `\\?\` 前缀,把它当普通路径 `open()` 失败。

**上游修复**:PR [#367](https://github.com/jhugman/uniffi-bindgen-react-native/pull/367) 用 `dunce::canonicalize` 自动剥前缀。**状态 APPROVED,但 0.31.0-2 之前未 release**。

**本地 patch**(`pnpm patch` 方式):

```bash
# 1. 展开包到临时目录
pnpm patch uniffi-bindgen-react-native

# 2. 在临时目录改 3 个文件:
#    crates/ubrn_common/Cargo.toml      — 加 `dunce = "1.0.5"`
#    crates/ubrn_common/src/files.rs    — pwd() 与 canonicalize_utf8_or_shim 用 dunce
#    crates/ubrn_common/src/rust_crate.rs — import Utf8PathExt + 把 canonicalize_utf8() 换成 _or_shim 版本

# 3. commit patch,写进 pnpm.patchedDependencies
pnpm patch-commit <临时目录>
```

`patches/uniffi-bindgen-react-native@0.31.0-2.patch` 进 git,`pnpm install` 自动 apply。

**不要**:

- `subst` 虚拟盘符:cargo 内部仍会 `canonicalize` 回真实盘符 + `\\?\`
- 跨平台 hack(改 `cargo target dir` 等):`std::fs::canonicalize` 在 Windows 永远加前缀

---

## 坑 3:`includesGeneratedCode: true` 陷阱(Android)

**症状**:第一次跑 `ubrn:android` 然后 `expo run:android`

```text
CMake Error at CMakeLists.txt:xxx:
  Cannot find source file: android/build/generated/source/codegen/jni/...
```

**原因**:ubrn 生成的 `package.json` 含

```json
{
  "codegenConfig": {
    "name": "MyLibSpec",
    "type": "modules",
    "outputDir": { "android": "android/generated" },
    "includesGeneratedCode": true                  // ← 这是陷阱
  }
}
```

`includesGeneratedCode: true` 告诉 RN codegen "我已经预生成好了,你别跑了"。但 ubrn 的 `CMakeLists.txt` 硬编码引用了 RN codegen 的输出路径(`build/generated/source/codegen/jni/...`),这个目录因为 codegen 没跑永远不存在。

**修复**:在 `package.json` 删 `includesGeneratedCode`(**保留** `outputDir`,bob 还要用):

```json
{
  "codegenConfig": {
    "name": "MyLibSpec",
    "type": "modules",
    "jsSrcsDir": "src",
    "outputDir": { "android": "android/generated" }
    // includesGeneratedCode 删除
  }
}
```

之后 `package.json` 加进 `noOverwrite`。

清缓存重跑:

```bash
rm -rf android/build android/generated
npx expo run:android
```

---

## 坑 4:`isNewArchitectureEnabled()` 缓存幻象(Android,RN 0.76+)

**症状**:构建成功 + 安装成功,但运行时

```text
Crash: TurboModuleRegistry.getEnforcing(...) 'MyLib' could not be found
```

**原因**:ubrn 生成的 `android/build.gradle` 有

```groovy
if (isNewArchitectureEnabled()) {
  apply plugin: "com.facebook.react"
  react { jsEngine = "hermes" }
}
```

RN 0.76+ New Architecture 默认开启;Expo SDK 55+ 永远开启不可关。但 Gradle 的条件判断**有时被缓存**:首次构建时 `isNewArchitectureEnabled()` 因为某些依赖还没加载返回 false,plugin 跳过,TurboModule 注册代码也跳过。下次构建条件正确了,但 plugin 注册的 task 已经被 cache 当 up-to-date 跳过。

**修复**:去掉 if 让 plugin **无条件** apply:

```groovy
apply plugin: "com.facebook.react"

react {
    jsEngine = "hermes"
}
```

注释说明:

```groovy
// New Architecture is always enabled in RN 0.76+ / Expo SDK 55+
apply plugin: "com.facebook.react"
```

清缓存重建:

```bash
rm -rf android/build .gradle
npx expo run:android
```

---

## 坑 5:`build.gradle` 模板年久失修(RN 0.83+ codegen 流程变)

**症状**(承接坑 3):删了 `includesGeneratedCode` 之后,Kotlin 编译仍报

```text
error: Cannot find source file: ...AndroidManifestNew.xml
error: Could not find NativeMyLibSpec class
```

**原因**:ubrn 的 Java 风格 `build.gradle` 模板基于 RN < 0.76 旧架构设计,RN 0.76+ codegen 流程已变。具体问题:

- 模板硬引用了 `src/main/AndroidManifestNew.xml`,但 ubrn 没生成它
- 模板手动把 `generated/java`、`generated/jni` 加入 sourceSets,与新版 codegen 插件自动加入冲突 → 类重复定义

**修复**:让 `com.facebook.react` 插件**全权管理 codegen**,移除手动配置。

`build.gradle` 两处改:

```groovy
// 改动 1:移除 manifest 文件引用
if (supportsNamespace()) {
  namespace "com.mylib"
  // 移除整个 sourceSets { main { manifest.srcFile "..." } } 块
}

// 改动 2:移除手动 generated 目录追加
sourceSets {
  main {
    // 不写任何 java.srcDirs 追加
  }
}
```

`package.json` 同步移除 `outputDir`(让 codegen 写默认路径 `build/generated/source/codegen/jni/`):

```json
{
  "codegenConfig": {
    "name": "MyLibSpec",
    "type": "modules",
    "jsSrcsDir": "src",
    "android": { "javaPackageName": "com.mylib" }
    // 删 outputDir / includesGeneratedCode
  }
}
```

不需要 `react-native.config.js`——autolinking 走默认 `cmakeListsPath` 即可。

---

## 坑 6:iOS podspec 兼容性代码导致 module 找不到

**症状**:`pod install` 失败,或 build 成功但运行时

```text
error: module 'MyLib' not found
```

**原因**:ubrn 生成的 podspec 含旧架构兼容分支

```ruby
if respond_to?(:install_modules_dependencies)
  install_modules_dependencies(s)
else
  s.dependency "React"            # 旧路径,与 New Arch 冲突
  s.dependency "React-Core"
  # ...
end
```

现代 RN + Expo 项目 `install_modules_dependencies` 总是存在,但 podspec 的旧分支被部分 hook(podspec parse 顺序)误读。

**修复**:删 if/else,只保留新方式:

```ruby
Pod::Spec.new do |s|
  s.name          = 'MyLib'
  s.version       = '0.1.0'
  s.platform      = :ios, '17.0'                   # 见坑 7
  s.source        = { :path => '.' }
  s.source_files  = 'ios/**/*.{h,m,mm,swift}'
  s.frameworks    = ['Foundation']                  # 见坑 8

  install_modules_dependencies(s)                   # 现代 RN 总是有这个方法
end
```

---

## 坑 7:iOS `___chkstk_darwin` 链接失败

**症状**:`ubrn:ios` cargo build 阶段

```text
ld: warning: object file was built for newer 'iOS' version (26.x) than being linked (10.0)
Undefined symbols for architecture arm64:
  "___chkstk_darwin", referenced from: _xxx in libyyy.rlib
```

**原因**:

- `rustc` 的 `aarch64-apple-ios` target 默认链接器 deployment target = **iOS 10**
- `cc` crate 用当前 Xcode SDK 编 C 依赖(libsqlite3-sys / ring / blake3 等)→ iOS 26.x SDK
- iOS 13 才引入的 `___chkstk_darwin` 被新编 C obj 引用,而 Rust 侧选 iOS 10 → 符号缺失

**修复**:crate 级 `.cargo/config.toml`(`<your-crate>/.cargo/config.toml`):

```toml
[env]
IPHONEOS_DEPLOYMENT_TARGET = "17.0"
```

`IPHONEOS_DEPLOYMENT_TARGET` 同时影响 `cc` crate 和 `rustc`——两边统一到同一个 min version,不再错配。

与 app 的 `ios.deploymentTarget` 对齐(Expo: `expo-build-properties` 的 `ios.deploymentTarget = "17.0"`)。

**不要**:

- 改成 `10.0` 把 C 警告压下去——新 SDK 里 iOS 10 已不可用
- 只设临时 shell env(重开终端就丢)→ 写进 crate 级 config 更稳
- 写 15.1 / 14.0 这种"以前能用"的旧值——2025-04 起 Apple 强制 App Store 提交 target iOS 17+ SDK

---

## 坑 8:`SystemConfiguration` 等系统 framework 缺失

**症状**:Rust 静态库链进 app 时报

```text
Undefined symbols for architecture arm64:
  "_SCPreferencesCreate", referenced from: ...
  "_kSCNetworkInterfaceTypeIEEE80211", referenced from: ...
```

**原因**:Rust 依赖链(典型链路 `libp2p-tcp → if-watch → system-configuration`)的 build.rs 会 emit `cargo:rustc-link-lib=framework=SystemConfiguration`,但 **cargo 在 `crate-type = ["staticlib"]` 下故意丢弃所有 transitive build metadata**([cargo#4814](https://github.com/rust-lang/cargo/issues/4814),2017 年的 design limitation,上游不会修)。

`libxxx.a` 里只有对 `_SC*` 符号的引用,没有 `-framework SystemConfiguration` 指令。app 链接时找不到。

**修复**:podspec 显式声明:

```ruby
s.frameworks = ["Foundation", "Security", "SystemConfiguration", "Network", "CoreFoundation"]
```

Signal(libsignal)、Mozilla application-services 都这么做,业界通用。ubrn 的 `ubrn.config.yaml` 也没有 `ios.frameworks` 字段可用,只能手写 podspec。

**怎么发现需要哪些 framework**:加新 Rust crate 后跑一次:

```bash
cargo build --target aarch64-apple-ios --message-format=json 2>/dev/null \
  | grep -oE 'rustc-link-lib=framework=[A-Za-z]+' | sort -u
```

输出里每个 `framework=X` 对照 podspec 的 `s.frameworks`,缺的补上,podspec 里用注释标出由哪条 crate 链触发,方便未来 review。

**常见嫌疑**:`libp2p` / `rustls` / `tokio-native-tls` / `ring` 这个组合可能用到 `SystemConfiguration` / `Security` / `Network` / `CoreFoundation`。按实际链接报错增加,不预防性全加。

---

## 坑 9:Xcode 26+ `SwiftUICore.tbd "not an allowed client"`

**症状**:`expo run:ios` / `xcodebuild` 链接 app target 时

```text
Could not parse or use implicit file '.../SwiftUICore.framework/SwiftUICore.tbd':
cannot link directly with 'SwiftUICore' because product being built is not an allowed client of it
```

**原因**:Xcode 16+(含 Xcode 26)的 linker 对 Apple 私有子 framework `SwiftUICore.tbd` 有 `allowable-clients` 白名单,**只对 iOS 17+ target 开放**。SDK 里 CocoaPods 插入的 implicit autolink 会拖它进来,deployment target < 17 就被拒。

跟 ubrn 本身无关,但因为我们跑 `ubrn:ios` 产出 xcframework,链接阶段才会暴露。

**修复**:

1. 装 `expo-build-properties`:`npx expo install expo-build-properties`
2. `app.json` plugins 加:

   ```json
   ["expo-build-properties", {
     "ios": { "deploymentTarget": "17.0" }
   }]
   ```

3. crate 级 `.cargo/config.toml` 的 `IPHONEOS_DEPLOYMENT_TARGET` 同步到 `17.0`(见坑 7)
4. `npx expo prebuild --platform ios --clean`
5. `pnpm ubrn:ios` 重编 xcframework,验证 `LC_BUILD_VERSION minos = 17.0`:

   ```bash
   otool -l target/aarch64-apple-ios-sim/debug/libxxx.a | grep -A3 LC_BUILD_VERSION | head
   ```

**为什么不用 `-weak_framework SwiftUICore` hack**:零星社区做法用 Podfile `post_install` 注入 `-weak_framework SwiftUICore` 压住错误,但 Expo/RN 都没官方背书;运行到 < iOS 17 设备上如果真调 SwiftUICore 会 silent crash。Apple 2025-04 起已强制 App Store 提交 target iOS 17+ SDK,这不是临时绕过、是大方向。

---

## 坑 10:cargo 拉私有 submodule 鉴权失败

**症状**:`cargo build` 拉私有 git 依赖时

```text
failed to authenticate when downloading repository
attempted to find username/password via git's `credential.helper` support, but failed
```

**原因**:cargo 内置 libgit2 不走 git CLI,拿不到 macOS keychain / SSH agent 的凭据。

**修复**:全局 `~/.cargo/config.toml`:

```toml
[net]
git-fetch-with-cli = true
```

让 cargo 调系统 `git` CLI 拉远程,`credential.helper = osxkeychain` 和 SSH key 才生效。一份配置覆盖所有 cargo 项目。

---

## 坑 11:Rust panic `no reactor running` (async 默认 runtime 误用)

**症状**:JS 第一次 `await someAsyncMethod()`

```text
[Error: Rust panic]
# adb logcat 里:
panic: there is no reactor running, must be called from the context of a Tokio 1.x runtime
```

**原因**:`#[uniffi::export] impl` 块含 `async fn`,但**没标 `async_runtime = "tokio"`**。uniffi 默认用 futures executor,不是 tokio reactor,`tokio::fs` / `sea-orm` / `reqwest::Client` 等依赖 tokio reactor 的 future 一调就 panic。

**修复**:

```rust
#[uniffi::export(async_runtime = "tokio")]    // ← 必加
impl MyClient {
    pub async fn fetch(&self, url: String) -> Result<String, MyError> { ... }
}
```

`Cargo.toml` 也要开 feature:

```toml
uniffi = { version = "0.31", features = ["cli", "build", "tokio"] }
```

模块级 async fn 同理。所有含 async 方法的 impl 块都要标。

---

## 坑 12:react-native.config.js 残留干扰

**症状**:头文件找不到

```text
fatal error: 'MyLibImpl.h' file not found
```

**原因**:从 create-react-native-library 模板继承的 `react-native.config.js`:

```javascript
module.exports = {
  project: { ios: {}, android: {} },
  // 可能还有 cxxModuleName / cxxModulePackageName
};
```

它告诉 Metro 去 ubrn 还没生成的位置找 C++ 头文件。

**修复**:**直接删整个文件**。ubrn 生成的 CMake / Pod 配置会自己处理头文件搜索路径。

---

## 坑 13:CMakeLists 的 `require.resolve` 在 pnpm workspace + CI 下返回空 → include 塌成 `/cpp/includes`

**症状**:本地 `ubrn:android` + gradle 全绿,但 **CI(GitHub Actions)** 的 `Gradle assembleRelease` 挂在

```text
fatal error: 'UniffiCallInvoker.h' file not found
Task :react-native-xxx-core:buildCMakeRelWithDebInfo[arm64-v8a] FAILED
```

clang 命令行里 include 路径是绝对的 **`-I/cpp/includes`**(注意开头那个 `/`),不是 `node_modules/.../cpp/includes`。

**原因**:ubrn 生成的 `android/CMakeLists.txt` 用

```cmake
execute_process(COMMAND node -p "require.resolve('uniffi-bindgen-react-native/package.json')"
                OUTPUT_VARIABLE UNIFFI_BINDGEN_PATH ...)
```

`execute_process` **没有 `WORKING_DIRECTORY`**,默认 cwd 是 Gradle 的深层 `.cxx` build 目录。本地 cwd 恰好在能 resolve 的位置所以过;CI 下从 build 目录向上找不到 `node_modules`(pnpm workspace 结构 + build 目录位置),`require.resolve` 抛错、stdout 空 → `UNIFFI_BINDGEN_PATH` 空 → `${UNIFFI_BINDGEN_PATH}/cpp/includes` 塌成 `/cpp/includes`。且 execute_process 没设 `RESULT_VARIABLE`,空值被静默吞掉。

**修复**(改 CMakeLists + noOverwrite 保护):

1. `execute_process` 加 `WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}"`——强制从 CMakeLists 所在目录跑 node,向上稳定找到 node_modules。
2. 加空值兜底:`node-linker=hoisted` 下包恒在 monorepo `mobile/node_modules`,从 `packages/<lib>/android` 上溯三级即 `mobile/`:

   ```cmake
   if(UNIFFI_BINDGEN_PATH)
     get_filename_component(UNIFFI_BINDGEN_PATH "${UNIFFI_BINDGEN_PATH}" DIRECTORY)
   else()
     set(UNIFFI_BINDGEN_PATH "${CMAKE_CURRENT_SOURCE_DIR}/../../../node_modules/uniffi-bindgen-react-native")
   endif()
   ```

3. `ubrn.config.yaml` 的 `noOverwrite` 加 `android/CMakeLists.txt`,否则下次 `--and-generate` 覆盖回原样。

**为什么本地测不出来**:本地 gradle build 的 cwd / node_modules 布局与 CI 不同(尤其 pnpm workspace + `expo prebuild --clean` 之后),require.resolve 本地能过、CI 挂。改完只能靠 CI 重验。SwarmDrop 2026-07 首次用 ubrn 0.31.0-3 发 mobile-v0.8.0 时踩到,桌面 v0.8.0 已发、移动因此挂。

---

## 完整修复后的标准状态

跑 `ubrn:android` / `ubrn:ios` 不再报错。检查清单:

```yaml
# ubrn.config.yaml
noOverwrite:
  - android/build.gradle      # 坑 4, 5
  - package.json              # 坑 3, 5
  - "*.podspec"               # 坑 6, 8
  - CMakeLists.txt
```

```toml
# rust/<crate>/.cargo/config.toml
[env]
IPHONEOS_DEPLOYMENT_TARGET = "17.0"     # 坑 7, 9
```

```toml
# ~/.cargo/config.toml (全局,一次配)
[net]
git-fetch-with-cli = true               # 坑 10
```

```rust
// 所有含 async 的 impl 块
#[uniffi::export(async_runtime = "tokio")]   // 坑 11
impl Foo { ... }
```

```bash
# 文件清单(都要存在或不存在)
✅ scripts/fix-ubrn-output.mjs              # 坑 1
✅ patches/uniffi-bindgen-react-native@*.patch  # 坑 2(仅 Windows)
✅ package.json 含正确 ubrn:* scripts
❌ react-native.config.js                    # 坑 12,不要
```

## 上游什么时候修?

按现状跟踪:

| 坑 | 上游状态 | 跟踪点 |
|----|----------|--------|
| 1. async static | 0.31.0-2 仍有 | `crates/ubrn_bindgen/.../templates/macros.ts` 的改动 |
| 2. Windows `\\?\` | PR #367 APPROVED,未 merge | release notes 提到 `dunce` |
| 3. includesGeneratedCode | 0.31.0-2 仍生成 | `package.json` 模板更新 |
| 5. build.gradle 模板年久失修 | 部分修了(Kotlin 风格模板更新),Java 风格未修 | `crates/ubrn_cli/templates/build.kt.gradle` 风格切换 |

修了的话回头删掉对应 patch / fix 脚本,简化项目。

## 相关
- [setup.md](setup.md) — `ubrn.config.yaml` 与 `noOverwrite` 完整字段
- [async-and-callbacks.md](async-and-callbacks.md) — `async_runtime = "tokio"` 的细节
- [multi-crate-and-publish.md](multi-crate-and-publish.md) — 跨 crate 的 wrap 模式
