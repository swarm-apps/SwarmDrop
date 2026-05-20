# Toolchain

## 概览

pnpm workspace（仅 `packages/swarmdrop-core` 一个子包）+ Expo SDK 55 + Metro + Biome + Lingui +
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

### lightningcss / uniffi-bindgen-react-native 锁版本

`package.json` 用 `pnpm.overrides` 锁 `lightningcss: 1.30.1`（NativeWind v5 preview 的 ABI
要求），`pnpm.patchedDependencies` 给 `uniffi-bindgen-react-native@0.31.0-2` 打了 patch（修
Windows 路径 canonicalize）。升级这两个之前先验证 nativewind/ubrn 兼容性。

**相关文件**：[package.json](../../package.json), [patches/](../../patches/)

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

## Expo / Prebuild

### android/ 和 ios/ 在根目录是 prebuild 产物（已入 git）

仓库选择把 prebuild 出的原生壳入 git（`.gitignore` 用 `/android/` `/ios/` 只忽略根级，但实际
android/ios 目录在 git 跟踪下）—— `packages/*/android` 和 `packages/*/ios` 是 ubrn 的原生
桥接代码，必须入 git。详细原生构建流程见 [dev-notes/native-build.md](../native-build.md)。

**相关文件**：[.gitignore](../../.gitignore), [dev-notes/native-build.md](../native-build.md)
