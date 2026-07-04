# Toolchain

## 概览

pnpm workspace（仅 `packages/swarmdrop-core` 一个子包）+ Expo SDK 56 + Metro + Biome + Lingui +
TypeScript。原生壳走 `expo prebuild`（android/ios 在仓库根，工作流见
[dev-notes/native-build.md](../native-build.md)）。Rust 子包用 cargo + uniffi-bindgen-react-native。

## pnpm / 依赖

### .npmrc 必须 node-linker=hoisted

Metro 不识别 pnpm 默认的 symlink node_modules 结构，找不到 transitive 依赖。`.npmrc` 顶层强制
`node-linker=hoisted` 让 pnpm 改成扁平结构（类似 npm/yarn classic）。

**正确做法**：保持 `.npmrc: node-linker=hoisted` 不动。

**不要做**：删除 `.npmrc` 或改成 `isolated` —— Metro 会一边 resolve 失败一边 silent，看似启动
正常但 require runtime 报 `Unable to resolve module`。

**相关文件**：[.npmrc](../../.npmrc)

### 更新 SwarmDrop core rev 用定向 cargo update,别全量

把 `mobile-core/Cargo.toml` 里四个 git 依赖(swarmdrop-core/entity/migration/swarm-p2p-core)的
`rev` 指到 develop 新 SHA 后,**只跑定向更新**:

**正确做法**：
```bash
# 1. 改 Cargo.toml 四处 rev(develop 最新 SHA:git ls-remote <repo> develop)
cargo update -p swarmdrop-core -p entity -p migration -p swarm-p2p-core \
  --manifest-path packages/swarmdrop-core/rust/mobile-core/Cargo.lock 所在目录
cargo check --manifest-path .../mobile-core/Cargo.toml   # 验证 Rust 侧兼容
```

**不要做**：直接跑无参 `cargo update` —— 它会贪婪升级**整棵依赖树**,把 `sea-orm`(Cargo.toml 用
`"2.0.0-rc"` 松范围)从可编译的 `rc.38` 顶到 `rc.41`,后者 `query_all_raw` 的 trait 签名不兼容
(`E0053: expected Send future, found non-Send`)直接编译失败。踩到就 `git checkout -- Cargo.lock`
还原再定向更新。

**core 加字段会触发 drift guard**：`device.rs` / `events.rs` 用穷尽解构做 drift guard,develop 加了
新字段(如 `DeviceReceivePolicy.allow_mcp_accept_from_device`)会让 `cargo check` 报 `E0027/E0063`。
移动端不需要的字段:解构侧 `field: _` 忽略、反向构造侧给安全默认 —— 这样**不改 uniffi Record、不用
重生成 bindings**;要在移动端暴露该字段才需镜像 + `build:ios/android`(见下条 bindings 刷新)。

**相关文件**：[packages/swarmdrop-core/rust/mobile-core/Cargo.toml](../../packages/swarmdrop-core/rust/mobile-core/Cargo.toml),
[packages/swarmdrop-core/rust/mobile-core/src/device.rs](../../packages/swarmdrop-core/rust/mobile-core/src/device.rs)

### lightningcss / uniffi-bindgen-react-native 锁版本

`package.json` 用 `pnpm.overrides` 锁 `lightningcss: 1.30.1`（NativeWind v5 preview 的 ABI
要求），`pnpm.patchedDependencies` 给 `uniffi-bindgen-react-native@0.31.0-2` 打了 patch（修
Windows 路径 canonicalize）。升级这两个之前先验证 nativewind/ubrn 兼容性。

**相关文件**：[package.json](../../package.json), [patches/](../../patches/)

### UniFFI 接口变更后必须刷新 iOS xcframework

`packages/swarmdrop-core/SwarmdropCoreFramework.xcframework/**/*.a` 被 `.gitignore` 忽略（静态库
太大），所以 Rust / UniFFI 接口更新后，tracked 的 C++/TS 绑定可能已经变新，但本机 iOS 静态库仍
是旧的。`pnpm ios` 链接阶段如果报 `Undefined symbols for architecture arm64`，且缺失符号形如
`_uniffi_swarmdrop_mobile_core_fn_method_mobilecore_*`，优先怀疑这个问题。Xcode 26 输出里的
`SwiftUICore.tbd` / MetalToolchain search path warning 可能只是噪声，真正符号列表在
`.expo/xcodebuild.log`。

**正确做法**：
- 改了 `packages/swarmdrop-core/rust/mobile-core` 的 UniFFI 暴露接口后，先跑
  `pnpm --filter react-native-swarmdrop-core build:ios`，再跑 `pnpm ios`。
- 需要确认时，用 `xcrun nm -gU packages/swarmdrop-core/SwarmdropCoreFramework.xcframework/.../libswarmdrop_mobile_core.a`
  检查缺失的 `uniffi_swarmdrop_mobile_core_*` 符号是否已进入静态库。

**不要做**：
- 不要只看 `git status` 里 generated C++/TS 绑定是否有 diff；ignored `.a` 才是 iOS linker 实际需要的
  Rust 产物。

**相关文件**：`packages/swarmdrop-core/package.json`,
`packages/swarmdrop-core/ubrn.config.yaml`,
`packages/swarmdrop-core/SwarmdropCoreFramework.xcframework`

## Metro / Babel

### Metro 加 .po/.pot extensions 让 lingui catalog 可 import

`metro.config.js` 把 `po`/`pot` 加到 `sourceExts`，并用 `@lingui/metro-transformer/expo` 转译
catalog 文件。新增语言文件后无需手动注册。

**相关文件**：[metro.config.js](../../metro.config.js)

### React Compiler 只对 src/ 启用，避免污染 generated

`babel.config.js` 给 `babel-preset-expo` 传 `react-compiler.sources`，只对路径前缀
`<projectRoot>/src/` 的文件跑 compiler。`packages/*/src/generated/*` 是 ubrn 自动生成的，
compiler 介入会破坏 ubrn 的 turbo-module 形状。

**正确做法**：新增需要 compiler 的代码放 `src/` 下即可。

**相关文件**：[babel.config.js](../../babel.config.js)

## Biome / Format / Lint

### lint 命令只覆盖 src/，packages/*/src 走子包自己的脚本

根目录 `pnpm lint` 等价于 `biome check src/`，**不**包含 `packages/`。子包的
`packages/swarmdrop-core` 用 `react-native-builder-bob` 出 lib，单独跑 `typecheck`。biome.json
的 `files.includes` 同时含 `packages/**`，所以手动 `biome check` 时会扫子包，但 pnpm script
不触发。

**相关文件**：[biome.json](../../biome.json), [package.json](../../package.json)

## Lingui

### 改 Trans / t / msg 后跑 i18n:extract

新增/改翻译键后用 `pnpm i18n:extract` 重新抽取 catalog（带 `--clean` 删除孤立条目）。CI 没自动
跑这一步，提交前自己做。

**相关文件**：[lingui.config.ts](../../lingui.config.ts)

### memo 组件里经全局 i18n._ 解析的字符串要靠 useLingui() 订阅才会随语言切换

`@lingui/core` 的全局 `i18n` 就是 LinguiProvider 注入的同一个单例,非 React 层
(如 `src/core/transfer-types.ts` 的 `policyActionLabel`)可以直接 `i18n._(msg)` 解析——
不需要把 translate 回调层层注入(试过,纯冗余间接层,已删)。但 `memo` 组件在 render
期间调用这类函数得到的字符串不会自动随 locale 更新:组件里要调一次 `useLingui()`
(来自 `@lingui/react`,非 macro 版)建立 context 订阅,locale 切换才会穿透 memo 触发重算。
`<Trans>` 自己订阅 context,只有 render 期间手动解析的字符串需要这个。

**相关文件**：[src/components/activity-projection-card.tsx](../../src/components/activity-projection-card.tsx)、
[src/core/transfer-types.ts](../../src/core/transfer-types.ts)

## Maestro / E2E

### iOS 26 + Expo SDK 56 / RN 0.85 Fabric 下暂缓 Maestro selector 测试

截至 2026-06-25，iOS 26.x 模拟器上 `pnpm ios` 可以正常构建和运行 SwarmDrop，但 Maestro
`2.6.1` 无法稳定看到 React Native 子树：截图里 UI 已渲染，XCUITest/Maestro hierarchy 里却只剩
app root/window/native overlay，`testID`、可见文本、placeholder、`accessibilityLabel` 都可能匹配
不到。这和上游 open issue 的复现一致：Expo SDK 56 / RN 0.85.3 / New Architecture(Fabric) /
iOS 26.x 下 visible RN UI missing from Maestro/XCUITest accessibility tree。

**正确做法**：
- Android Maestro 仍作为当前可执行的移动 E2E smoke gate。
- UI foundation 的最小 Android gate 是
  `maestro --device emulator-5554 test .maestro/smoke/mobile-foundation.yaml`：
  覆盖设备 / 收件箱 / 设置三 tab、收件箱空态，以及从设备页进入二级活动页。
  这个 flow 只验证导航壳和空态边界；完整设备信任策略、Inbox 详情/删除、网络发现运行态、接收 offer
  等业务 E2E 归后续 OpenSpec changes 增量补齐。
- iOS 侧保留测试脚本和 testID，但 selector 型 Maestro flow 暂不作为必过 gate；需要 iOS 视觉确认时
  只做临时 `takeScreenshot` / `assertScreenshot` 或人工检查。
- 恢复 iOS Maestro 前，先确认
  <https://github.com/mobile-dev-inc/Maestro/issues/3367> 和
  <https://github.com/react/react-native/issues/57282> 已有官方修复或明确 workaround，再重跑
  `pnpm ios` + `maestro --device <ios-udid> test .maestro/smoke/onboarding.yaml`。

**不要做**：
- 不要通过排除 `@expo/ui`、`expo-symbols`、`expo-glass-effect` 等原生模块来“修” Maestro hierarchy；
  这只是绕开症状，还会改变真实 app surface。
- 不要指望在 SDK 56 里设置 `newArchEnabled: false` 规避 Fabric；Expo SDK 55+ / RN 0.82+
  已不支持关闭 New Architecture，该配置会被忽略。
- 不要把 selector flow 全量改成坐标点击后当成稳定 E2E；坐标只能作为临时截图采样手段。

**相关文件**：[.maestro/smoke/onboarding.yaml](../../.maestro/smoke/onboarding.yaml),
[.maestro/smoke/mobile-foundation.yaml](../../.maestro/smoke/mobile-foundation.yaml),
[package.json](../../package.json)

## SwarmHive 更新

### 下载 APK 后先校验 HTTP 与 ZIP magic

Android 自动更新的安装入口只会告诉用户“安装包解析失败”，不会解释下载内容是否真是 APK。
阿里云 OSS 原始 endpoint 会拦截 APK 公网分发并返回 XML：
`ApkDownloadForbidden: The APK file is not allowed to be distributed in a public network using the OSS endpoint, please use CNAME instead.`

**正确做法**：
- `expo-downloader` 下载完成后必须检查 HTTP status、文件存在且非空、文件头是 ZIP/APK magic
  (`PK...`)，失败时删除缓存文件并抛下载错误。
- SwarmHive 服务端使用阿里云 OSS 分发 APK 时，不要把客户端重定向到
  `*.oss-cn-*.aliyuncs.com/*.apk` 原始域名；应走 OSS CNAME 域名或服务端代理。

**不要做**：
- 不要把任意下载结果直接交给 PackageInstaller；XML/HTML 错误页也会被保存成 `.apk`，最后只剩
  系统级“解析失败”。

**相关文件**：[src/lib/expo-downloader.ts](../../src/lib/expo-downloader.ts),
[src/components/update-host.tsx](../../src/components/update-host.tsx)

## Expo / Prebuild

### android/ 和 ios/ 在根目录是 prebuild 产物（已入 git）

仓库选择把 prebuild 出的原生壳入 git（`.gitignore` 用 `/android/` `/ios/` 只忽略根级，但实际
android/ios 目录在 git 跟踪下）—— `packages/*/android` 和 `packages/*/ios` 是 ubrn 的原生
桥接代码，必须入 git。详细原生构建流程见 [dev-notes/native-build.md](../native-build.md)。

**相关文件**：[.gitignore](../../.gitignore), [dev-notes/native-build.md](../native-build.md)

### 文件预览的原生依赖选型（RN 0.85 new-arch 实测）

收件箱「打开」走系统预览：iOS 用 `react-native-file-viewer`（QLPreviewController），
Android 不用它——自拼 `ACTION_VIEW`（SAF `content://` 直接用，`file://` 经
`expo-file-system/legacy` 的 `getContentUriAsync` 转 content://）。

**实测结论**：
- RNFV 2.1.5 是 old-arch 模块，在 RN 0.85 / Expo SDK 56（new arch）经 interop 层
  **编译与运行都正常**（QuickLook 正常弹出）。若未来升级挂掉，兜底方案是
  `modules/quick-look/` 本地 expo-module（~50 行 Swift 直呈 QLPreviewController），
  接口保持 `open(path)`。
- RNFV 需要**解码后的绝对路径**：`decodeURIComponent(uri.replace(/^file:\/\//, ""))`，
  中文文件名在 file:// URI 里是 percent-encoded 的，直接传会找不到文件。
- `getContentUriAsync` 只在 `expo-file-system/legacy` 子路径（新 API 无等价物），
  调用集中在 `src/lib/open-file.ts` 单点，SDK 移除时换新 API。
- 图片全屏用 `react-native-image-viewing`（纯 JS，零原生成本）；视频内联用官方
  `expo-video`（SDK 配套版本，config plugin 自动加入 app.json）。

**不要做**：不要用 RNFV 的 Android 端（不认 SAF content://）；ACTION_VIEW 不要显式
setType（resolver 会向 provider 查 MIME，type+data 同设有兼容坑）。

**相关文件**：[src/lib/open-file.ts](../../src/lib/open-file.ts),
[openspec/changes/inbox-file-preview/design.md](../../openspec/changes/inbox-file-preview/design.md)
