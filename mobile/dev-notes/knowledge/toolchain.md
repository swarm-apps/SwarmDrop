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

`pnpm-workspace.yaml` 用 `overrides` 锁 `lightningcss: 1.30.1`（NativeWind v5 preview 的 ABI
要求）。ubrn 的 Windows 路径 patch 已随 0.31.0-3（上游 PR #367）合入删除；仍需的 Node-24
exports 兼容修复走 `.pnpmfile.cjs`（readPackage 给 ubrn 补 `./package.json` 导出）。
升级这两个之前先验证 nativewind/ubrn 兼容性。

**相关文件**：[pnpm-workspace.yaml](../../pnpm-workspace.yaml), [.pnpmfile.cjs](../../.pnpmfile.cjs)

### ubrn ≥0.31.0-3：生成代码 import `@ubjs/core`，子包必须显式声明该依赖

ubrn 0.31.0-3 起 TypeScript runtime 拆分为独立 npm 包 `@ubjs/core`（上游 PR #399，
uniffi-bindgen-javascript 改名的一部分）。重新生成绑定后，`src/generated/*.ts` 的
runtime import 从 `uniffi-bindgen-react-native` 变为 `@ubjs/core`——ubrn 本身**不再
把它作为传递依赖带进来**（ubrn 的 dependencies 为空），Metro 会直接报
`Unable to resolve "@ubjs/core"`。

**正确做法**：
- `packages/swarmdrop-core/package.json` 的 `dependencies` 里精确锁
  `"@ubjs/core": "<与 devDependencies 的 ubrn 完全同版本>"`（预发布版本线，caret 语义不可靠）。
- 升级 ubrn 时两处版本一起动，然后 `pnpm install` + 重新生成绑定。
- 忘了同步不会哑掉：`.pnpmfile.cjs` 在安装期校验两者版本相等，失配直接 throw。
- 上游改进已提 <https://github.com/jhugman/uniffi-bindgen-react-native/issues/414>
  （建议 ubrn 自己校验/声明 @ubjs/core 联动）——落地后本地校验可退役。

**历史包袱已清**：0.31.0-2 及以前需要 `scripts/fix-ubrn-output.mjs` 后处理产物里的
`async static` 生成 bug（TS 语法错误），0.31.0-3 起上游已修、脚本已删（见 git history）。
若未来升级 ubrn 后 bob build / typecheck 报 `async static` 语法错误，说明 bug 回归了——
去上游报 issue，别急着复活本地脚本。

**相关文件**：[packages/swarmdrop-core/package.json](../../packages/swarmdrop-core/package.json)

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

### Hermes 的 Intl 是残缺的 —— 用 `<Plural>` 前必须有 @formatjs polyfill

Hermes 虽然开了 Intl，但**只实现了四样**（真机 `Object.getOwnPropertyNames(Intl)` 实测原话）：

```
getCanonicalLocales,Collator,DateTimeFormat,NumberFormat
```

**既没有 `PluralRules`，也没有 `Locale`。** 而 `@lingui/core` v6 的 `plural()`
（`dist/index.mjs:75-84`）**无条件** `new Intl.PluralRules(...)` 且全文件零 try/catch，
于是任何 `<Plural>` / ICU plural 消息在**渲染期**抛
`TypeError: undefined cannot be used as a constructor` → dev 红屏、**release 硬闪退**
（全仓无 ErrorBoundary）。失败还不被 memoize（`getMemoized` 只在 construct 成功后才 cache），
所以是每次渲染都抛，不是只崩第一次。

**正确做法**：[src/i18n/polyfills.ts](../../src/i18n/polyfills.ts) 已配好，在
[src/i18n/lingui.ts](../../src/i18n/lingui.ts) 顶部（先于 `@lingui/core`）import：

```ts
import "@formatjs/intl-locale/polyfill-force.js";
import "@formatjs/intl-pluralrules/polyfill-force.js";
import "@formatjs/intl-pluralrules/locale-data/zh.js";
import "@formatjs/intl-pluralrules/locale-data/en.js";
```

**每一条都是踩出来的**：

- **两个 polyfill 缺一不可**。只补 `PluralRules` 会换个地方崩——它内部 locale 匹配要
  `new Intl.Locale(...)`（错在 `@formatjs/intl-localematcher` 的 `findMatchingDistanceImpl`）。
  Lingui 官方 RN 教程原文就是「polyfill `Intl.Locale` **and** `Intl.PluralRules`」。
  `intl-getcanonicallocales` 不需要（Hermes 原生有）。
- **`.js` 后缀不能省**——两个包的 exports map 只暴露带 `.js` 的路径，省掉解析不到。
- **zh + en 都要载**——lingui `normalizeLocales` 无条件追加 `"en"` 兜底，实际解析的 locales
  恒为 `[当前语言, "en"]`，少载任一个可能抛 `RangeError`。
- **用 `polyfill-force` 不用 `polyfill`**——后者的 native 探测在 Android 上拖慢启动秒级，
  而这里缺失是确定性的、不需要探测。
- 装完**清 Metro 缓存**（`expo start -c`）。

**踩坑史**：v0.7.16 前全项目只有 `app/device/groups.tsx` 一处 `<Plural>`（分组行设备数），
且只在设备数 ≥ 1 时渲染——设备数 0 走 `<Trans>` 碰不到 Intl，所以历次「真机验证」都没抓到，
直到用户把设备加进分组才必崩。

**验证手法**：复数**正确性**可以在 Node 里验（平台无关）——删掉 `Intl.PluralRules`/`Intl.Locale`
只留 Hermes 那四样，装 polyfill，跑真实 catalog；是否**崩**则必须上真机 A/B。

**相关文件**：[src/i18n/polyfills.ts](../../src/i18n/polyfills.ts)、
[src/i18n/lingui.ts](../../src/i18n/lingui.ts)、[src/app/device/groups.tsx](../../src/app/device/groups.tsx)

## iOS WebDriver 与 Appium 录屏

### iOS 端 E2E 改用 WebdriverIO + Appium XCUITest

iOS 26 / Expo SDK 56 / RN 0.85 Fabric 下 Maestro selector 不稳定时，iOS 端自动化走
WebDriver 协议：`pnpm e2e:ios` 会启动 Appium 3 + `appium-xcuitest-driver`，再由
WebdriverIO 执行 `e2e/webdriver/test/specs/*.e2e.ts`。

录制 demo 时继续使用同一个 Appium 会话录制屏幕，不要让 Maestro 另起一个 XCTest 会话：

- 外层脚本设置 `SWARMDROP_MOBILE_RECORDING_PATH` 后，demo spec 调用
  `browser.startRecordingScreen()`。
- 流程完成并收到外层 `mobile-close` 信号后调用 `browser.stopRecordingScreen()`，把返回的 base64 MP4
  写入指定路径。
- 双端传输入口会把移动端原始视频写到桌面仓库的
  `e2e/desktop/build/desktop-recordings/raw/ios-transfer-<timestamp>.mp4`。
- Appium XCUITest 录屏需要录制主机安装 `ffmpeg`；iOS 真机和模拟器都支持该接口，但录屏不包含音频。
- 真实设备的 WDA MJPEG 流固定使用 `10086` 端口；如果换机器或遇到端口占用，先检查该端口再修改
  `e2e/webdriver/wdio.ios.conf.ts`。本机验证可通过 `SWARMDROP_MJPEG_SERVER_PORT` 临时覆盖，并设置
  `SWARMDROP_APPIUM_SCREEN_RECORDING=1`。
- iOS Simulator 默认不启动 WDA MJPEG，使用 `simctl io recordVideo --display=1`，避免 WDA
  broadcaster 和 Simulator 视频捕获互相冲突；真实设备才使用 Appium `startRecordingScreen`。
- 传输 demo 只有在移动端出现 `transfer-success-state`，且桌面端出现 `send-success-state` 后才算成功；失败
  状态和超时都会让流程失败。

这种方式只录 iOS 设备画面，不包含 OBS 的桌面背景、模拟器窗口边框或 Maestro 的 Flow 面板，适合后续
交给 ffmpeg / Remotion 做裁切和双端合成。

**正确做法**：
- 选择器统一用 Accessibility ID：`await $("~onboarding-start-button").click()`。
- React Native 关键交互要补稳定 `testID`；必要时同步 `accessibilityLabel`。iOS 上 `testID`
  会映射到 accessibility identifier，是 Appium/XCUITest 的稳定锚点。
- 已安装 dev build 时直接 `SWARMDROP_IOS_UDID=<udid> pnpm e2e:ios`；需要安装指定 `.app` 时设置
  `SWARMDROP_IOS_APP_PATH=/path/to/SwarmDrop.app`。
- `appium driver install xcuitest` 在本仓库会被 `workspace:*` 依赖绊住，因为它内部调用 npm 修改当前包；
  用 pnpm 安装 `appium-xcuitest-driver` 到 devDependencies，Appium 3 可以从项目依赖中识别 driver。

**不要做**：
- 不要把 iOS Maestro 坐标点击当成稳定 gate；坐标只适合临时采样。
- 不要把 `appium-xcuitest-driver` 只装到全局环境后假定 CI/其他机器可用。

**相关文件**：[e2e/webdriver/wdio.ios.conf.ts](../../e2e/webdriver/wdio.ios.conf.ts),
[e2e/webdriver/test/specs/onboarding.e2e.ts](../../e2e/webdriver/test/specs/onboarding.e2e.ts),
[e2e/webdriver/README.md](../../e2e/webdriver/README.md)

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

### SAF localPath 必须来自 finalize_sink 返回值，不能拼接推导（vivo 真机实证）

v0.7.3 真机暴雷：SAF 目录下收件箱「打开」跳浏览器、「分享」直接 reject。根因是 core
落库的 `local_path` 用 `Path::join(saveDir, relativePath)` 字符串拼接——SAF 下拼出
`content://…/tree/<treeId>/<相对路径>` 的**伪 URI**（缺 `/document/<docId>` 段，
docId 有独立的 percent 编码），ContentResolver 解析不了：ACTION_VIEW 查不到 MIME
被浏览器兜底，openFileDescriptor 直接报错。重名冲突被系统改写成 "foo (1).jpg" 时
拼接推导同样失真（桌面端同理）。

**治本契约**：`FileAccess::finalize_sink -> AppResult<String>` 返回最终落盘位置
（桌面 = .part rename 后绝对路径；移动 = expo-fs `createFile` 返回的 `file.uri`），
receiver 经 `mark_file_completed` 写入 `transfer_files.local_path`，收件箱落库只认
它——NULL 显式报错，不做拼接推导回退。**不留历史数据兼容层**（用户拍板：当下没有
存量用户，旧库卸载重装即可），前端拿到的 localPath 保证合法可直接用。

**分享的平台边界**：expo-sharing 只接受 `file://`（SharingModule.kt 硬检查），SAF
文件走它必须整份 copy 进 cache（慢 + 膨胀）→ 弃用。expo-intent-launcher 的 extra
只能放基本类型，塞不进 `EXTRA_STREAM` 要的 Parcelable Uri。零拷贝方案 =
`modules/content-share/` 本地 expo-module（~30 行 Kotlin）：ACTION_SEND +
EXTRA_STREAM + `FLAG_GRANT_READ_URI_PERMISSION`，framework 的
migrateExtraStreamToClipData 负责把权限带给目标应用。MIME 从**原始文件名的扩展名**
经 `MimeTypeMap.getMimeTypeFromExtension` 查（**不要**用
`URLConnection.guessContentTypeFromName`：对含 `#` 的文件名如 "C#笔记.pdf" 有 AOSP
`StringIndexOutOfBoundsException` 崩溃 bug）。

**missing 是单向持久标记**：`markInboxFileMissing(...,true)` 落库且**无解除路径**
（`repairMissingInboxItems` 只补建丢失的收件箱条目，不清文件级 missing）。所以打开/
分享的存在性检查必须区分「明确查到不存在」(exists===false→判缺失) 与「查询本身抛错」
(provider 瞬时故障/授权未知→不判缺失，当普通错误让用户重试)——否则一次 SAF 抖动会
永久锁死好文件。`fileExists` 返回 `boolean | null` 表达三态。

**ubrn build --and-generate 会冲刷两端原生脚手架**：`pnpm build:ios` / `pnpm
build:android`（都带 `--and-generate`）会把 podspec/gradle 引用的手写模块源删掉、
生成一套示例 app 脚手架 —— iOS 删 `ios/SwarmdropCore.h/.mm` 生成 `ios/Podfile`+
`ios/reactnativeswarmdropcore*`；Android 把 `android/build.gradle` 换成 root-project
版、删 `CMakeLists.txt`/`cpp-adapter.cpp`/`AndroidManifest.xml`/`com/swarmdropcore/
*.kt`、生成 `android/app/` 示例 app。**坑**：被冲刷后本地构建可能因 gradle/pods 缓存
仍能装上（假绿），但干净构建 / CI release 必挂。
- **接口没变时**（如只改内部 Rust 实现、bump 依赖 rev）：用 `npx ubrn build
  android -t arm64-v8a` / `npx ubrn build ios`（**去掉 `--and-generate`**）——只重编
  Rust + 拷 jniLibs/xcframework，不碰脚手架、不重生成 bindings。这是首选。
- **接口变了必须 `--and-generate`**：跑完立刻 `git checkout -- packages/swarmdrop-core/
  {ios,android}/` + 删掉未跟踪的 `ios/Podfile*` `ios/reactnativeswarmdropcore*`
  `android/.gitignore` `android/app/`，然后 `rm -rf android/build android/.cxx` 干净
  重编确认。bindings 产物在 `cpp/generated`、`src/generated`（不在 ios/android/），
  checkout 脚手架不会丢它们。

### 只刷 bindings、不碰原生脚手架的更轻路径（接口变更时首选）

接口变了要重生成 bindings，但**不必**跑 `ubrn build --and-generate`（它会冲刷脚手架，见上条）。
把「重生成 bindings」与「重编原生」拆开：

**正确做法**：
```bash
# 1. 先 build 出 host dylib（供 uniffi 提取元数据，本机 target，快）
cd packages/swarmdrop-core/rust/mobile-core && cargo build
# 2. 只生成 TS + C++ bindings（library 模式，从 dylib 提取；CWD 需有 Cargo.toml）
#    dylib 路径见下方「合并进单仓后」的坑——不再是本目录下的 target/debug/
npx ubrn generate jsi bindings --library \
  --ts-dir ../../src/generated --cpp-dir ../../cpp/generated \
  --crate swarmdrop_mobile_core <dylib-path>/libswarmdrop_mobile_core.dylib
# 3. 需要能真机跑时，再 npx ubrn build ios / android -t arm64-v8a（无 --and-generate，
#    只重编 Rust + 拷 xcframework/jniLibs，不重生成、不碰脚手架）
```
- 纯新增 uniffi `Record`（如 `MobileFinalizedSink`）时 `cpp/generated` **无变化**（C++ 侧走通用
  RustBuffer 序列化），只有 `src/generated` 的 TS 类型变。diff 应干净、无杂散 churn。
- `ubrn generate jsi bindings` 在 CWD 跑 `cargo metadata`，必须在 `rust/mobile-core` 目录下跑，
  输出路径写成相对该目录的 `../../{src,cpp}/generated`。

### 合并进单仓后，dylib 产物路径变了——不在 `rust/mobile-core/target/`

上面那条录自 SwarmDrop-RN 还是独立仓库时——那时 `rust/mobile-core` 自己就是 workspace 根，
`cargo build` 的产物天然落在同目录 `target/debug/`。并入本仓后 `mobile-core` 是**根 Cargo
workspace 的 member**（`Cargo.toml:workspace.members` 含它，`cargo metadata --no-deps` 的
`root` 指向仓库根 `Cargo.toml`），`cargo build` 的产物因此落在**仓库根**的 `target/debug/`，
`rust/mobile-core/target/debug/libswarmdrop_mobile_core.dylib` 根本不存在——照抄旧命令会
`No such file or directory`。

**正确做法**：
```bash
# 在仓库任意位置 build 均可（同一个 workspace target 目录）
cargo build -p swarmdrop-mobile-core
cd mobile/packages/swarmdrop-core/rust/mobile-core   # ubrn 仍需在此目录找 Cargo.toml
<mobile>/node_modules/.bin/ubrn generate jsi bindings --library \
  --ts-dir ../../src/generated --cpp-dir ../../cpp/generated \
  --crate swarmdrop_mobile_core /Volumes/yexiyue/SwarmDrop/target/debug/libswarmdrop_mobile_core.dylib
```
- `ubrn` 可执行文件也跟着提升到 `mobile/node_modules/.bin/ubrn`（pnpm workspace 提升），
  不在 `packages/swarmdrop-core/node_modules/`。
- 找不准路径时 `find /Volumes/yexiyue/SwarmDrop/target/debug -iname "*mobile_core*"` 现查最快。

**相关文件**：`Cargo.toml`（根 workspace members）、
[packages/swarmdrop-core/rust/mobile-core/Cargo.toml](../../packages/swarmdrop-core/rust/mobile-core/Cargo.toml)

**相关文件**：[packages/swarmdrop-core/ubrn.config.yaml](../../packages/swarmdrop-core/ubrn.config.yaml)

### 改了 generated bindings 后，app 侧 typecheck 要先 `pnpm prepare`（bob build）

`react-native-swarmdrop-core` 的 `package.json` 里 `types` 指向 **built** 的
`lib/typescript/...`（不是 `src/`），而 `exports.source` 才指 `src/`。于是：
- **Metro（运行时）** 走 `source` → 直接吃新改的 `src/generated`，无需 build。
- **tsc（app typecheck）** 走 `types` → 吃 `lib/typescript/` 的**旧**声明。改完 `src/generated`
  必须 `cd packages/swarmdrop-core && pnpm prepare`（bob build 重建 lib）后，app 的 `tsc` 才看得到
  新字段（如 `MobileTransferProjection.contentRoot`），否则报「属性不存在」。

**相关文件**：[packages/swarmdrop-core/package.json](../../packages/swarmdrop-core/package.json)

**相关文件**：[src/lib/open-file.ts](../../src/lib/open-file.ts),
[modules/content-share/](../../modules/content-share/),
[src/core/foreign-file-access.ts](../../src/core/foreign-file-access.ts)
