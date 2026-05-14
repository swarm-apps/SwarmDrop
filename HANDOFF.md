# HANDOFF — extract-core-and-add-rn-mobile 收尾工作

> 这份文件是从 Windows 工作机切换到 macOS 后,让另一台机器上的 LLM(Claude / Codex / 等)
> 继续推进 `extract-core-and-add-rn-mobile` OpenSpec 变更的交接手册。
>
> **阅读完整本文档,再开始执行任何命令。**
> 仓库当前状态、目标产物、验收标准、上下文都在下面。

---

## 0. TL;DR(给 LLM 的一段话)

你接手的是一个跨平台 P2P 文件传输应用 SwarmDrop 的"抽离共享 Rust core + 新增 React Native 移动端"重构。
代码层面 96 个任务里 88 个已经在 Windows 机上完成(`openspec/changes/extract-core-and-add-rn-mobile/tasks.md` 可查),
**但是有三类工程产物没法在 Windows 上生成,必须在 Mac 上补齐**:

1. **iOS 原生壳** — UniFFI 生成的 `packages/swarmdrop-core/ios/` 目录、`SwarmdropCoreFramework.xcframework/` 预编译框架
2. **Android 原生壳** — `packages/swarmdrop-core/android/build.gradle` 等 RN autolink 入口文件
3. **iOS 实际构建验证** — 跑通一次 `expo run:ios` 或 EAS build,记录环境缺口

同时还有一批工程脚手架 / CI / 签名插件 / 应用图标等小项(P1/P2)落后于参考工程 `swarmnote-mobile`,
建议一并补齐。下面的 §3 是按优先级排好的清单。

参考工程位置(如果对方机器上有的话):
- 桌面参考: `swarmnote` (https://github.com/swarm-apps/SwarmNote.git)
- 移动参考: `swarmnote-mobile` (https://github.com/swarm-apps/SwarmNote-RN.git)

> ℹ️ 主仓库已从 `yexiyue/SwarmDrop` 迁移到 `swarm-apps/SwarmDrop`,本地 origin
> 还指向旧地址也能用(GitHub 自动重定向),但建议在 Mac 上执行
> `git remote set-url origin https://github.com/swarm-apps/SwarmDrop.git` 更新一下。

---

## 1. 仓库布局

这次重构涉及 **两个独立仓库**,分别 clone:

```
~/workspace/
├── swarmdrop/           # 主仓库,Tauri 桌面端 + Rust workspace(core/entity/migration)
│   └── (你正在读这个文件就是从这里来的)
└── swarmdrop-mobile/    # 独立 RN 仓库,Expo Router + UniFFI bridge
```

Clone 命令:

```bash
mkdir -p ~/workspace && cd ~/workspace

# 主仓库 — checkout 当前 WIP 分支
git clone https://github.com/swarm-apps/SwarmDrop.git swarmdrop
cd swarmdrop
git checkout wip/extract-core-and-rn-mobile-handoff
git submodule update --init --recursive
cd ..

# 移动端仓库
git clone https://github.com/swarm-apps/SwarmDrop-RN.git swarmdrop-mobile
```

> ⚠️ **submodule**: `swarmdrop/libs/` 是 P2P 库 `swarm-p2p-core`(独立 git submodule)。
> 不要忘记 `--recursive`。

### 1.1 swarmdrop 仓库结构

```
swarmdrop/
├── Cargo.toml                      # Rust workspace 根
├── crates/
│   ├── core/                       # swarmdrop-core (平台无关业务核心)
│   ├── entity/                     # SeaORM 实体
│   └── migration/                  # SeaORM migrations
├── libs/                           # submodule: swarm-p2p-core
├── src-tauri/                      # Tauri 桌面端(实现 Host trait)
│   └── src/host/                   # Desktop host 实现:KeychainProvider/EventBus/AppPaths/...
├── src/                            # React 桌面 UI
├── openspec/changes/
│   └── extract-core-and-add-rn-mobile/
│       ├── tasks.md                # ★ 进度跟踪
│       ├── proposal.md
│       ├── design.md
│       └── specs/
└── dev-notes/architecture/         # 新加的架构文档
```

### 1.2 swarmdrop-mobile 仓库结构

```
swarmdrop-mobile/
├── app.json                        # Expo 配置(还没加 icon/splash 图)
├── babel.config.js                 # ⚠ 多余的 worklets/plugin 待清理
├── metro.config.js
├── biome.json
├── packages/
│   └── swarmdrop-core/             # ★ UniFFI 包装的 RN 原生模块
│       ├── rust/mobile-core/       # Rust 源(依赖 path = "../../../../swarmdrop/crates/core")
│       ├── android/                # ⚠ 缺 build.gradle 等产物
│       ├── ios/                    # ⚠ 整目录都不存在
│       ├── SwarmdropCore.podspec   # 引用了不存在的 SwarmdropCoreFramework.xcframework
│       ├── cpp/generated/          # uniffi-bindgen 生成的 C++ 桥
│       ├── src/index.ts            # ★ 保留
│       ├── src/index.tsx           # ⚠ 冗余,需要删
│       ├── ubrn.config.yaml
│       └── package.json
├── src/                            # Expo Router 应用
│   ├── app/                        # 路由(11 个页面:onboarding/pairing/send/main)
│   ├── stores/                     # zustand(mobile-core/notification/onboarding/transfer)
│   ├── core/                       # Host trait 适配层:event-bus/file-access/keychain/...
│   ├── components/                 # pairing-request-host / transfer-offer-host / code-pairing-card
│   └── hooks/
└── patches/                        # uniffi-bindgen-react-native@0.31.0-2 patch
```

⚠ **重要**:`swarmdrop-mobile/packages/swarmdrop-core/rust/mobile-core/Cargo.toml` 中的
`swarmdrop-core` 路径是 `../../../../swarmdrop/crates/core`,即 **依赖同级目录的 swarmdrop**。
所以两个仓库必须 clone 到同一个父目录下(`~/workspace/swarmdrop` 和 `~/workspace/swarmdrop-mobile`)。

---

## 2. Mac 上要先准备的环境

```bash
# 1. Xcode + Command Line Tools — 必须
xcode-select --install
# 在 App Store 装 Xcode(用于 iOS build);打开一次接受协议

# 2. Rust 工具链(iOS + Android targets)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android

# 3. cargo-ndk(Android 构建用)
cargo install cargo-ndk

# 4. Node + pnpm(参考 swarmnote-mobile:pnpm 10 + node 22)
brew install pnpm node@22
pnpm --version  # 应该 >= 10

# 5. Android SDK + NDK(用于 Android build) — 建议装 Android Studio
brew install --cask android-studio
# 装完后在 Android Studio 里:SDK Manager → SDK Tools → NDK (Side by side) 选 r27c
# 然后设置环境变量(写进 ~/.zshrc):
#   export ANDROID_HOME=$HOME/Library/Android/sdk
#   export NDK_HOME=$ANDROID_HOME/ndk/27.x.x
#   export PATH=$PATH:$ANDROID_HOME/platform-tools

# 6. CocoaPods
brew install cocoapods

# 7. uniffi-bindgen-react-native (ubrn) CLI — 通过 pnpm 在 packages 内调用,不需要全局
# 8. Java 17(Android build 需要)
brew install openjdk@17
# 按 brew 的提示配置 JAVA_HOME

# 9. expo-cli 不需要全局,通过 pnpm exec 调用
```

验证环境:

```bash
xcodebuild -version       # Xcode 15+
rustc --version
cargo ndk --version
pod --version             # CocoaPods
java --version            # 17.x
adb --version             # platform-tools
```

---

## 3. 任务清单(按优先级排序)

`[ ]` = 待办;每项后面括号是估时 + 难度。Mac 必须项标 🍎。

### 3.1 P0 — 阻塞 release 链路,必须做

#### 3.1.1 🍎 生成并提交 swarmdrop-core 的 iOS 原生壳 (30min, Mac 必须)

```bash
cd ~/workspace/swarmdrop-mobile
pnpm install --frozen-lockfile

# 生成 iOS 产物(包括 ios/、SwarmdropCoreFramework.xcframework/、cpp/generated 等)
pnpm --filter react-native-swarmdrop-core build:ios
# 这一步会:
#   - 编译 Rust 到 aarch64-apple-ios + aarch64-apple-ios-sim + x86_64-apple-ios
#   - 用 ubrn 生成 ios/SwarmdropCore.{h,mm}
#   - 用 xcodebuild -create-xcframework 打包成 SwarmdropCoreFramework.xcframework
#   - 跑 scripts/fix-ubrn-output.mjs 修正生成产物
```

期望产物:

```
packages/swarmdrop-core/
├── ios/
│   ├── SwarmdropCore.h
│   └── SwarmdropCore.mm
├── SwarmdropCoreFramework.xcframework/
│   ├── Info.plist
│   ├── ios-arm64/...
│   └── ios-arm64_x86_64-simulator/...
```

验证:

```bash
ls packages/swarmdrop-core/ios/*.{h,mm}
ls packages/swarmdrop-core/SwarmdropCoreFramework.xcframework/Info.plist
```

#### 3.1.2 🍎 生成并提交 swarmdrop-core 的 Android 原生壳 (15min, 可在 Mac/Linux/WSL)

```bash
cd ~/workspace/swarmdrop-mobile
pnpm --filter react-native-swarmdrop-core build:android
# 这一步会生成:
#   - android/build.gradle           ← 最关键!RN autolink 入口
#   - android/generated/             ← 生成的 Kotlin/Java glue
#   - android/src/main/jniLibs/      ← .so 库(arm64-v8a 等)
```

验证:

```bash
test -f packages/swarmdrop-core/android/build.gradle && echo OK
ls packages/swarmdrop-core/android/src/main/jniLibs/arm64-v8a/
```

#### 3.1.3 删除冗余文件 (1min)

```bash
cd ~/workspace/swarmdrop-mobile
rm packages/swarmdrop-core/src/index.tsx
# 保留 src/index.ts,package.json 中 exports.source 也指向 ts
```

#### 3.1.4 修正 swarmdrop-mobile/.gitignore (5min)

当前 `.gitignore` 把 `android/` `ios/` 完全忽略,会让 packages/swarmdrop-core 下的产物也被忽略。
改成只忽略 mobile root 的 expo prebuild 产物:

```diff
- android/
- ios/
+ /android/
+ /ios/
```

验证:

```bash
git check-ignore -v packages/swarmdrop-core/ios/SwarmdropCore.h
# 应该输出"无匹配"(返回码 1)
```

#### 3.1.5 补 swarmdrop-mobile 应用图标和 splash (30min)

参考 `swarmnote-mobile/assets/`,准备:

- `assets/images/icon.png`(1024×1024)
- `assets/images/adaptive-icon.png`(1024×1024,Android)
- `assets/images/splash.png`(1284×2778)
- `assets/images/favicon.png`(48×48)
- `assets/images/android-icon-{foreground,background,monochrome}.png`(可选,Android 13+)

然后在 `app.json` 中引用:

```jsonc
{
  "expo": {
    "icon": "./assets/images/icon.png",
    "android": {
      "package": "com.yexiyue.swarmdrop",
      "predictiveBackGestureEnabled": false,
      "adaptiveIcon": {
        "foregroundImage": "./assets/images/adaptive-icon.png",
        "backgroundColor": "#0F172A"
      }
    },
    "plugins": [
      "expo-router",
      "expo-secure-store",
      [
        "expo-splash-screen",
        {
          "image": "./assets/images/splash.png",
          "imageWidth": 200,
          "backgroundColor": "#0F172A"
        }
      ],
      ...
    ]
  }
}
```

如果暂时没有设计稿,可以先从 `swarmdrop/src-tauri/icons/` 里挑一张(已有的桌面图标)放大处理,
或者用 ImageMagick 生成纯色占位图先跑通构建:

```bash
mkdir -p assets/images
convert -size 1024x1024 xc:'#0F172A' -fill white -gravity center -pointsize 200 \
        -annotate +0+0 'SD' assets/images/icon.png
cp assets/images/icon.png assets/images/adaptive-icon.png
convert -size 1284x2778 xc:'#0F172A' assets/images/splash.png
convert -size 48x48 xc:'#0F172A' assets/images/favicon.png
```

#### 3.1.6 🍎 验证 iOS 构建(任务 11.9) (60min, Mac 必须)

```bash
cd ~/workspace/swarmdrop-mobile
pnpm expo prebuild --platform ios --clean
cd ios && pod install && cd ..
pnpm expo run:ios
# 或更轻量:pnpm expo run:ios --device 'iPhone 15 Pro' (模拟器)
```

记录:

- 启动成功 → 在 `openspec/changes/extract-core-and-add-rn-mobile/tasks.md` 把 `11.9` 勾上
- 启动失败 → 把错误日志摘要写进 tasks.md 的 11.9 行后面(标记本机环境缺口)

#### 3.1.7 🍎 验证移动端 ↔ 桌面端配对 + 双向传输(任务 11.4 / 11.5 / 11.6) (90min)

需要:Windows 那台桌面机也在线,两端通过 Internet 配对。
- 启动桌面 SwarmDrop(Windows / Mac 都行,`pnpm tauri dev`)
- 启动移动 SwarmDrop(iOS 模拟器或真机)
- 走分享码配对 → 双向发送 1MB 文件

成功后勾掉 tasks.md 的 11.3 / 11.4 / 11.5 / 11.6;失败则记录 reproduction。

---

### 3.2 P1 — 协作 / CI / 发版链路,强烈建议补

#### 3.2.1 桌面端补 biome.json 和 CI (30min, 任意平台)

桌面 `swarmdrop/` 完全没有 lint 工具,参考 `swarmnote/biome.json`:

```bash
cd ~/workspace/swarmdrop
# 直接抄 swarmnote 的(如果对方机器上也 clone 了):
# cp ../swarmnote/biome.json .
# 否则从 swarmnote 仓库 raw 下载
```

然后在 `package.json` scripts 加:

```json
{
  "scripts": {
    "lint": "biome check .",
    "lint:ci": "biome ci .",
    "format": "biome check --write ."
  },
  "devDependencies": {
    "@biomejs/biome": "^2.4.10"
  }
}
```

再加 `.github/workflows/ci.yml`(参考 `swarmnote/.github/workflows/ci.yml`)。

#### 3.2.2 桌面端修复 crate edition 矛盾 (2min)

```diff
# swarmdrop/crates/core/Cargo.toml
- edition = "2021"
+ edition.workspace = true
```

`crates/entity/Cargo.toml` 和 `crates/migration/Cargo.toml` 同理改。
然后 `cargo check --workspace` 确认通过。

#### 3.2.3 桌面端修 release.yml 中文乱码 (5min)

`swarmdrop/.github/workflows/release.yml` 中 `workflow_dispatch.inputs.min_version.description` 是
Windows GBK 错编进 utf-8 的乱码:

```yaml
description: '寮哄埗鏇存柊鏈€浣庣増鏈彿 ...'
```

改成正确的:

```yaml
description: '强制更新最低版本号 (如 0.2.0),留空则不设置'
```

文件全文重新过一遍中文,确认没有其他乱码。

#### 3.2.4 移动端补 CI workflow (30min, 任意平台)

复制 `swarmnote-mobile/.github/workflows/build-android.yml` 和 `release.yml` 到
`swarmdrop-mobile/.github/workflows/`,然后改:

- 包名:`swarmnote` → `swarmdrop`
- editor-core sibling clone 步骤删除(swarmdrop 没有 editor)
- bundle 名 / artifact 名 / repo url

#### 3.2.5 移动端补 release signing + install permission plugin (20min)

从 swarmnote-mobile 复制:

```
swarmnote-mobile/plugins/with-android-release-signing.js   → swarmdrop-mobile/plugins/
swarmnote-mobile/plugins/with-android-install-permission.js → swarmdrop-mobile/plugins/
```

然后在 `app.json` 的 `expo.plugins` 加上:

```json
[
  ...,
  "./plugins/with-android-release-signing",
  "./plugins/with-android-install-permission"
]
```

#### 3.2.6 移动端清理 babel.config.js (1min)

```diff
// swarmdrop-mobile/babel.config.js
- plugins: ["react-native-worklets/plugin"]
+ plugins: []
```

Reanimated 4.x 已经不需要该插件,留着会冲突。

#### 3.2.7 移动端 biome.json schema 升级 (2min)

```diff
- "$schema": "https://biomejs.dev/schemas/2.2.0/schema.json"
+ "$schema": "https://biomejs.dev/schemas/2.4.10/schema.json"
```

跟 `@biomejs/biome` 装的实际版本对齐;参考 swarmnote-mobile 的 overrides 配置。

#### 3.2.8 同步依赖版本 (5min)

`swarmdrop-mobile/package.json`:

```diff
- "@react-native-async-storage/async-storage": "2.1.2"
+ "@react-native-async-storage/async-storage": "2.2.0"

- "react-native-worklets": "0.7.4"
+ "react-native-worklets": "0.7.2"
```

然后 `pnpm install`,跑 `pnpm tsc --noEmit` 确认 OK。

---

### 3.3 P2 — 协作规范,有时间就做

- 桌面 + 移动两端补 `.editorconfig` + `.gitattributes`(从 swarmnote 复制)
- 桌面补 `pnpm-workspace.yaml`(包含 docs)+ `lefthook.yml` + `commitlint.config.js`
- 移动补 `.gitignore` 加 `.expo/` `.metro-health-check*` `*.tsbuildinfo` `*.jks` `*.p12` `*.key` `*.mobileprovision`
- 移动 `metro.config.js` 留好扩展点(可参考 swarmnote-mobile,只保留 watchFolders 那一段以备未来引入 i18n)

---

## 4. 完成后的提交策略

每个 P0 子项一个 commit,在 swarmdrop / swarmdrop-mobile 分别:

```bash
# 例子(在 swarmdrop-mobile 仓库)
git checkout -b feat/mobile-native-artifacts
git add packages/swarmdrop-core/ios/ packages/swarmdrop-core/SwarmdropCoreFramework.xcframework/
git commit -m "feat(swarmdrop-core): 生成 iOS 原生壳与预编译 xcframework"

git add packages/swarmdrop-core/android/build.gradle packages/swarmdrop-core/android/generated/ packages/swarmdrop-core/android/src/main/jniLibs/
git commit -m "feat(swarmdrop-core): 提交 Android UniFFI 构建产物"

git add .gitignore
git commit -m "fix(mobile): .gitignore 只忽略 mobile root 的原生壳"

# ... 等

git push origin feat/mobile-native-artifacts
# 然后开 PR
```

桌面 swarmdrop 的 wip commit(目前在分支 `wip/extract-core-and-rn-mobile-handoff`)
最后回到 `develop` 之前,**建议 squash 成一个语义化 commit 或拆成 5-7 个**。
具体可以请用户决定。

---

## 5. 验收标准(OpenSpec archive 前要满足的)

打开 `openspec/changes/extract-core-and-add-rn-mobile/tasks.md`,最终需要满足:

- [ ] 11.1 — 桌面端首次启动无密码
- [ ] 11.2 — 旧 Stronghold 数据切到 keychain
- [ ] 11.3 — 桌面 ↔ 桌面配对传输
- [ ] 11.4 — 桌面 ↔ 移动配对
- [ ] 11.5 — 桌面 → 移动小文件
- [ ] 11.6 — 移动 → 桌面小文件
- [ ] 11.7 — 移动端前后台切换网络恢复
- [x] 11.8 — Android 构建通过(已在 Windows 验证)
- [ ] 11.9 — iOS 构建通过(本次 Mac 任务的核心)
- [x] 11.10 — lint / check / test 通过(已在 Windows 验证)

全部勾掉后:

```bash
cd ~/workspace/swarmdrop
pnpm exec openspec archive extract-core-and-add-rn-mobile
# 或人工把 openspec/changes/extract-core-and-add-rn-mobile/ 移到 openspec/archive/
```

---

## 6. 重要上下文(对方 LLM 必读)

1. **语言**:用户要求所有响应、commit message、注释、文档使用简体中文。技术名词保留原文。
2. **架构原则**:
   - `crates/core` 不依赖任何 Tauri / Expo / 平台特定 crate
   - 平台相关行为通过 Host trait 注入(`KeychainProvider` / `EventBus` / `AppPaths` /
     `FileAccess` / `Notifier` / `UpdateInstaller`)
   - 桌面 host 在 `swarmdrop/src-tauri/src/host/`,移动 host 在
     `swarmdrop-mobile/src/core/`(JS 适配层)+ `swarmdrop-mobile/packages/swarmdrop-core/rust/mobile-core/src/host/`(Rust 桥)
3. **代码风格**:
   - 不写不必要的注释。注释只写 *为什么*,不写 *做了什么*
   - 不为假设的未来需求引入抽象。三行重复好过过早抽象
   - 错误处理只在系统边界做,内部代码相信类型契约
4. **Git 安全**:
   - 不要 force push / 不要 amend 已推送的 commit / 不要跳过 hook
   - libs/ 是 submodule,**不要主动更新它的 commit pointer**
5. **不要做的事**:
   - 不要主动添加 v1 兼容代码或 fallback 路径
   - 不要主动给 README / docs / CHANGELOG 加内容(除非用户要求)
   - 不要修任何 `openspec/specs/` 下已存在的文件
6. **OpenSpec 工具**:可用 `pnpm exec openspec --help` 查看;不熟悉时优先读
   `openspec/AGENTS.md` 或 `openspec/changes/extract-core-and-add-rn-mobile/proposal.md`

---

## 7. 卡住时

- iOS build 链接错误 / duplicate symbols → 看 `swarmdrop-mobile/packages/swarmdrop-core/SwarmdropCore.podspec`
  里的 `s.source_files` 是否多写了 `cpp/generated/**`(应该不该有,RN 0.83 的 ReactCodegen 已经统一了那个)
- iOS pod install 找不到 SystemConfiguration → podspec 必须显式声明
  `s.frameworks = "SystemConfiguration"`,因为 libp2p 的 if-watch crate 依赖它,
  rlib 合并成 static lib 时 autolink 指令会丢
- Android build.gradle 没生成 → 确认 `pnpm --filter ...` 跑的是 `build:android --and-generate`
  而不是 `build:android` 单独;`--and-generate` 是关键
- 找不到 path 依赖 swarmdrop-core → 确认两个仓库 clone 在同一父目录下(`~/workspace/swarmdrop`
  和 `~/workspace/swarmdrop-mobile`)
- ubrn 命令找不到 → `pnpm install` 后 ubrn 会进 `node_modules/.bin/`,用 `pnpm exec ubrn` 调

---

最后:**遇到不确定的地方,先 ask 用户,再动手**。这个项目已经走到 88/96,
最后这一里路要稳。祝顺利 🚀
