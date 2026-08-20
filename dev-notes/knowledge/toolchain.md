# Toolchain

## 概览

构建 / 包管理 / lint / CI 的项目特有约束。常规命令参考 CLAUDE.md "Build and Development Commands"；本主题只记非显见的坑。

## `DESIGN.md` 的契约层要靠门禁守，不能靠人看

`e7d9caee`（2026-08-14）重新生成 `DESIGN.md` 时把手写的跨端契约层整层覆盖掉了
（1274 行 → 186 行，10 个 `### … Contract` 全没）。**六天没有人发现**——因为损害不是报错，
而是 `CLAUDE.md` 与十几处代码注释里的引用**指向了不存在的东西**：读到那些注释的人会去
找判据，找不到，然后凭感觉实现。

这个文件分两层，混在一起是它被冲掉的原因：

| 层 | 谁维护 | 能不能重新生成 |
|---|---|---|
| 视觉令牌（frontmatter / Colors / Typography / Components） | `/impeccable` 系列命令 | 能 |
| `## Cross-platform Contracts` | 手写 | **不能** |

`pnpm check:design-contracts` 守两条：契约节数不低于 `MIN_CONTRACTS`（整层被冲掉时归零），
以及**全仓每一处 `Xxx Contract` 引用都指得到某个 `### ` 标题**（匹配按前缀，于是
`Node Status Contract` 对得上 `### Node Status Contract (cross-platform)`）。
两种故障都实测过会红并返回退出码 1。

⚠️ **它刻意不管 `… Rule`**：`One Accent Rule` 这类在同一次重新生成里被中文重写成了
「**单一强调色规则**」，引用它们的注释同样悬空。那属于「token 层要不要保留英文命名」，
与「手写判据别被工具冲掉」不是一回事——一起管会让门禁第一次运行就红，然后被人加进忽略清单。


## 包管理

### pnpm only

项目锁定 pnpm 11（`packageManager` 字段 + pnpm-lock.yaml）。npm / yarn install 会产生不一致的 lockfile。

### pnpm 11 settings 放在 pnpm-workspace.yaml —— `.npmrc` 与 `package.json` 的 `pnpm` 字段都不再读

pnpm 11 不再把 `package.json` 里的 `pnpm` 字段作为项目 settings 来源，**`.npmrc` 里的项目级
settings 同样不读**。overrides、build script 白名单、依赖 patch、node-linker 等，都要放到对应
项目根的 `pnpm-workspace.yaml`。

**正确做法**：
- 根桌面应用用仓库根 `pnpm-workspace.yaml`，当前仅声明 `packages: ["."]` 和允许 `esbuild` build script。
- 独立 e2e 子项目用 `e2e/desktop/pnpm-workspace.yaml`，把 WDIO 相关 overrides 和 build-script 策略放在那里。
- `mobile/pnpm-workspace.yaml` 是完整样例（迁移自 RN 独立仓的 pnpm 10 配置）。

**不要做**：
- 在 `package.json` 里新增 `pnpm.overrides` / `pnpm.onlyBuiltDependencies`，pnpm 11 下会被忽略。

#### 迁移时按「会不会吭声」分类，哑的那些优先查

移动端从 pnpm 10 并入时四项配置全部失效，但**只有一半会报错**：

| 原位置 | pnpm 11 归宿 | 失效表现 |
|---|---|---|
| `pnpm.overrides` | `overrides:` | **响** — `ERR_PNPM_LOCKFILE_CONFIG_MISMATCH`，frozen 装不上 |
| `pnpm.onlyBuiltDependencies` | `allowBuilds:` | **响** — `ERR_PNPM_IGNORED_BUILDS`，要求逐个表态 |
| `pnpm.patchedDependencies` | `patchedDependencies:` | **哑** — patch 不打，直接拿未打补丁的包用 |
| `.npmrc` 的 `node-linker=hoisted` | `nodeLinker: hoisted` | **哑** — 静默退回 isolated |

两个哑的都很致命：

- **`patchedDependencies`** — 本项目给 ubrn 打了 patch（加 `dunce` 依赖）。不生效则拿未打补丁
  的 ubrn 去生成绑定。验证方式：`node_modules/.pnpm/` 下的目录名带 `_patch_hash=<hash>` 后缀，
  且该 hash 与 lockfile 里 `patchedDependencies` 记录的一致。
- **`nodeLinker`** — RN 的 Metro bundler 不认 symlink(isolated) 结构，必须 hoisted。失效后
  顶层包数 847 → 70，传递依赖（`expo-modules-core` 等）不在顶层，typecheck 与 Metro 双双解析
  失败。**判据是顶层包数（`ls node_modules | wc -l`），不是有没有 `.pnpm` 目录** —— hoisted 下
  `.pnpm` 作为 store 照样存在。

`allowBuilds` 语义也变了：pnpm 10 的 `onlyBuiltDependencies` 是白名单（未列出的静默不跑），
pnpm 11 要求每个都显式 `true`/`false`，否则报错。等价迁移要把当年被隐式忽略的依赖显式写成 `false`。

**相关文件**：`package.json`、`pnpm-workspace.yaml`、`mobile/pnpm-workspace.yaml`、`e2e/desktop/pnpm-workspace.yaml`

### 给 mobile 加依赖：`pnpm add` 撞 `ERR_PNPM_UNUSED_PATCH`，绕过时当心连带 regen uniffi 绑定

pnpm 11.10 下在 `mobile/` 里 `pnpm add <任何包>`（含 `expo install`）会报
`[ERR_PNPM_UNUSED_PATCH] uniffi-bindgen-react-native@0.31.0-2`——patch 目标是嵌套成员
`packages/swarmdrop-core` 的依赖，从 mobile 根视角被判「未使用」，哪怕版本精确匹配、patch 文件在、
`_patch_hash` 也对。（pnpm 11.15+ 可能已修，但别为装一个包动全局 pnpm 版本。）

**加包做法**：手动把依赖写进 `mobile/package.json`，再
`pnpm install --config.allowUnusedPatches=true`。该 flag 只作用本次调用、不落进
`pnpm-workspace.yaml`，不改仓库配置。装完 `grep` 一下 `pnpm-lock.yaml` 确认 patch 条目仍在、
新包已入锁、无无关包版本变更（大 diff 多是 peer-dep hash churn，同版本重写属正常）。

**必须复查的副作用（危险）**：`allowUnusedPatches` 有可能让 ubrn 的 patch 不打，随后
prepare/codegen 用未打补丁的 ubrn **重新生成 uniffi 绑定**，把
`mobile/packages/swarmdrop-core/{cpp,src}/generated/*`（6000+ 行）一并改了。这些是生成物、
和你装的包无关，`git add mobile/` 会把它们一起 stage 进功能提交。**装完包务必 `git status`
看有没有 generated 冒出来**；只要你没动 FFI 接口（`crates/core` / `mobile-core` 的 Rust 签名），
committed 的绑定就是对的，直接
`git checkout HEAD -- mobile/packages/swarmdrop-core/cpp/generated mobile/packages/swarmdrop-core/src/generated`
回退，别让它混进提交。真要 regen 绑定是独立动作，走 `pnpm --filter react-native-swarmdrop-core build:ios`（patch 正常应用）。

**相关文件**：`mobile/pnpm-workspace.yaml`、`mobile/packages/swarmdrop-core/**/generated/`

### 改了 FFI 签名要 regen 绑定：不必跑 `build:ios`（不需要 Xcode，快一个数量级）

`pnpm --filter react-native-swarmdrop-core build:ios` 会为两个 iOS target 交叉编译 Rust，
分钟级起步且要 Xcode。但**只是想让改过的 `#[uniffi::export]` 反映到 TS 绑定**时不需要它——
ubrn 支持从任意一个已编好的动态库里提取定义：

```bash
# ① 编本机 target（几十秒，dev profile）
cargo build -p swarmdrop-mobile-core

# ② 从 .dylib 提取并重新生成 TS + C++ 绑定
#    注意 cwd 必须是 crate 目录：ubrn 会在 cwd 跑 `cargo metadata`，
#    在 packages/swarmdrop-core 下跑会报 "manifest path `Cargo.toml` does not exist"
cd mobile/packages/swarmdrop-core/rust/mobile-core
pnpm exec ubrn generate jsi bindings --library \
  --ts-dir ../../src/generated --cpp-dir ../../cpp/generated \
  /Volumes/yexiyue/SwarmDrop/target/debug/libswarmdrop_mobile_core.dylib

# ③ 生成物在 src/generated，但 JS 侧 import 的是包的 lib/——必须再 build 一次，
#    否则 `pnpm typecheck` 会报 "has no exported member 'MobileXxx'"，
#    看起来像绑定没生成，其实是 lib/ 还是旧的
cd /Volumes/yexiyue/SwarmDrop/mobile && pnpm --filter react-native-swarmdrop-core prepare
```

「No prettier found」「Skipping formatting C++」是正常提示，产物照常写出。真机构建仍要走
`build:ios` / `build:android`，本路径只覆盖「绑定与类型」这一层。

⚠️ **`lib/` 是 gitignore 的，所以第 ③ 步的产物进不了提交**——但这不会让 CI 红：
`mobile-build-android.yml` 根本没有 `pnpm typecheck` 这一步，它 `pnpm install` 之后直接跑
`build:android`（会重建整条桥接）。也就是说「绑定陈旧」只在**本地**表现为 typecheck 报
「找不到某个新方法」，CI 上看不见。反过来说：CI 绿**不能**证明本地 regen 步骤做对了。

**相关文件**：`mobile/packages/swarmdrop-core/ubrn.config.yaml`

### 官网 Hero 视频使用独立 Remotion 工程

`video/` 是用于制作官网成片的独立 pnpm workspace，不参与桌面应用或 `docs/` 的依赖安装。Remotion
只在本地 Studio 和导出时运行；官网静态导出只消费 `docs/public/hero/` 内的 MP4 与封面图。

**正确做法**：
- 进入 `video/` 后使用 `pnpm studio` 预览、`pnpm render:hero` 导出。
- 保持成片尺寸 1920 × 1080、30 fps、20 秒；网页播放用静音、循环的原生 `<video>`。
- 使用 `useCurrentFrame()`、`interpolate()`、`spring()` 表达视频时间线，不要在 Remotion Composition 中使用 CSS 动画。

**不要做**：
- 不要把 `remotion` 或 `@remotion/player` 加进 `docs/package.json`，也不要在 GitHub Pages 构建中渲染视频。
- 不要提交未经裁剪的原始录屏；`video/out/` 为本地临时产物。

**相关文件**：`video/`、`docs/public/hero/`、`docs/app/(home)/page.tsx`

### 前端测试使用 Vitest

前端单元/组件测试使用 Vitest + jsdom + Testing Library，配置集中在 `vitest.config.ts`，复用
React Lingui macro Babel 插件和 `@` alias。测试文件放在 `src/**/*.test.ts(x)`，执行：

```bash
pnpm test
```

**正确做法**：
- 需要 mock generated bindings 时 mock `@/lib/bindings`，不要改 `src/lib/bindings.ts`
- Zustand store 测试里 mock `@/lib/tauri-store`，避免 Tauri plugin store 在 Node 环境初始化
- Lingui 组件测试用 `src/test/setup.ts` 激活测试 locale，组件测试按需包 `I18nProvider`

**相关文件**：`vitest.config.ts`、`src/test/setup.ts`、`src/stores/network-store.test.ts`

### 桌面端 E2E 用 WebdriverIO，跟 Vitest 是两套独立体系

`e2e/desktop` 是用官方 `npm create wdio@latest e2e/desktop` 向导生成的独立项目，native 模式
驱动真实 Tauri 二进制，跟 `pnpm test`（Vitest，管 `src/**/*.test.ts(x)` 单元/组件测试）不是
一回事，也不应该合并——WDIO 生态没有 vitest framework adapter，只有 `@wdio/mocha-framework`
等，看到 `wdio.conf.ts` 里 `framework: 'mocha'` 是正常的，不是配置错误。`vitest.config.ts`
已显式 `exclude: [...configDefaults.exclude, "e2e/**"]` 避免 glob 误扫（`exclude` 会整体覆
盖默认值，必须展开 `configDefaults.exclude` 而不是只写新增项）。

**正确做法**：加新 E2E 能力用官方 CLI 重新生成/调整，不要手写 `wdio.conf.ts`；这个向导目前
生成的配置本身有几处已知 bug（`services` 数组多一个不存在的 `'tauri-plugin'` service、
`capabilities` 还是浏览器 boilerplate、`@wdio/native-utils` 版本对不上导致运行时报错）——
详见 [`dev-notes/blogs/desktop-webdriver-e2e.md`](../blogs/desktop/desktop-webdriver-e2e.md) 的
"常见坑"。

**最容易踩的一个坑**：native 二进制必须用 `pnpm tauri build --debug --no-bundle` 构建，裸
`cargo build` 出来的二进制会因为 `tauri.conf.json` 的 `devUrl` 指向没启动的 Vite dev server
而白屏（窗口标题读出来是空字符串，`window.__TAURI__.core.invoke` 一直超时）。

### 首次启动的空白窗口也要检查 Zustand 持久化合并

`main.tsx` 会等待 preferences-store hydration 完成才渲染路由。Tauri Store 的文件即使存在，
也可能还没有 `preferences-store` 这个 key；这时 Zustand 会把 `persistedState` 传为 `undefined`。
若 `merge` 直接访问它的字段，hydration 会在内部 catch 后停住，`hasHydrated()` 永远为 false，
窗口则因 `return null` 持续空白。

**正确做法**：持久化合并函数先把缺失状态归一为 `{}`，例如
`const persisted = (persistedState ?? {}) as Partial<PreferencesState>`；并在排查时用
Tauri MCP Bridge 或 DevTools 检查 `usePreferencesStore.persist.hasHydrated()`。

**相关文件**：`src/stores/preferences-store.ts`、`src/main.tsx`

**相关文件**：`e2e/desktop/`、`dev-notes/blogs/desktop-webdriver-e2e.md`

### 桌面端官网素材录制用 WDIO demo spec + OBS WebSocket

`e2e/desktop` 里 demo spec 与常规 E2E 分开：

- 常规 E2E：`test/specs/**/*.e2e.ts`，由 `pnpm --dir e2e/desktop wdio` 执行。
- 录制 demo：`test/specs/demo/*.demo.ts`，由 `record-desktop-demo.mjs` 按场景单独执行。

录制入口：

```bash
pnpm --dir e2e/desktop record desktop-home
pnpm --dir e2e/desktop record send-file
pnpm --dir e2e/desktop record inbox
# 单次启动应用，连续录制首页、发送入口、收件箱三段主片
pnpm --dir e2e/desktop record desktop-suite
```

默认会连接 `OBS_WEBSOCKET_URL=ws://127.0.0.1:4455`，不显式设置
`OBS_WEBSOCKET_PASSWORD` 时会读取 macOS OBS 配置里的 `server_password`。OBS 必须已启用
WebSocket Server 且有屏幕录制权限。脚本会先构建 `pnpm tauri build --debug --no-bundle`，
再启动 WDIO，让 demo spec 等到桌面首页稳定后写入 ready 信号；脚本收到 ready 后才
`StartRecord`，短暂延迟后写入 go 信号放行交互，最后 `StopRecord`。这样录制文件只包含真实
demo 操作，不包含 Tauri/WebDriver 启动等待。manifest 和 raw clip 会写到
`e2e/desktop/build/desktop-recordings/`；关键截图写到 `e2e/desktop/build/wdio/screenshots/`。

**正确做法**：
- demo flow 用稳定 `data-testid`，不要靠文案或复杂 CSS 层级。
- 录制模式下 demo helper 会先 `switchWindow("main")`，避免 `@wdio/tauri-service` 每个
  focus 命令都触发 5 秒 `Tauri core.invoke` fallback。
- OBS 短视频停止后要等输出文件大小稳定再复制；太早复制容易得到 0B 或 moov 不完整的视频。
- demo spec 串行录制。当前 Tauri WebDriver 使用固定端口，并行跑多个 native demo 会抢端口。
- 批量录制桌面基础素材时使用 `desktop-suite`，它在一个 WDIO worker 中依次加载首页、发送入口、收件箱场景；Tauri 应用只会冷启动一次，并会在全部场景结束后由 WDIO 正常收尾。需要单独补录时，继续使用三个单场景命令。
- `send-file.demo.ts` 允许没有在线已配对设备：此时只输出首页/空环境素材，不把录制管线判失败。
- 录制产物在 `build/` 下，仓库根 `.gitignore` 已忽略，不要提交视频原始文件。

**相关文件**：`e2e/desktop/scripts/record-desktop-demo.mjs`、`dev-notes/blogs/desktop-demo-recording-pipeline.md`

录制平台选择、当前 Android / iOS 验证结论和产物约定见 [demo-recording.md](demo-recording.md)。

### 双端 WebDriver composite 录制入口

桌面 + iOS 双端素材使用 `pnpm --dir e2e/desktop record:transfer` 作为外层 orchestrator。它会启动
`../SwarmDrop-RN` 的 Metro dev server，然后并行跑桌面 WDIO demo flow 和 RN `pnpm e2e:ios`。
桌面端由 OBS 录制 Tauri 窗口，移动端由同一个 Appium/XCUITest WebDriver 会话录制 iOS 设备画面。
桌面 flow 会先等 iOS 真正上线，若检测到上一轮残留配对，会通过
`browser.tauri.execute(({ core }) => core.invoke("remove_paired_device", ...))` 清掉旧 peer，再等右侧附近设备
回到"可配对"状态；两个端都 ready 后，外层脚本启动 OBS，写入 go 信号放行交互，录完再统一关闭。

**正确做法**：
- OBS 里预先布好桌面 Tauri 窗口采集；移动端不要再通过 OBS 采集，避免把模拟器外框和桌面背景带进素材。
- iOS 端选择器走 `../SwarmDrop-RN/e2e/webdriver` 的 Appium/XCUITest；录屏仍由同一个 WebDriver 流程
  生命周期控制。Simulator 使用 `simctl recordVideo`，真实设备设置 `SWARMDROP_APPIUM_SCREEN_RECORDING=1`
  后调用 Appium `startRecordingScreen` / `stopRecordingScreen`。真实设备 WDA MJPEG 端口固定为 `10086`。
- 移动端原始视频写入 `build/desktop-recordings/raw/ios-transfer-<timestamp>.mp4`；Appium 录屏需要录制主机安装
  `ffmpeg`，iOS 真机从 iOS 11 起支持该接口，且不包含音频。
- 桌面端和移动端都必须进入成功状态后才结束 demo；任一端进入失败状态或超时，整个录制任务失败。
- 调试脚本本身用 `pnpm --dir e2e/desktop record:transfer --no-record --skip-build`，避免启动 OBS 和 Appium 录屏。
- `pnpm tauri build --debug --no-bundle` 是生产 Vite build，`import.meta.env.DEV` 为 false。录制构建必须带
  `VITE_WDIO_TAURI_PLUGIN=1`，否则前端不会加载 `@wdio/tauri-plugin`，`browser.tauri.execute` 会报
  `Tauri core.invoke not available after 5s timeout`。
- 演示节奏用 `SWARMDROP_DEMO_STEP_DELAY_MS` 控制，默认 1000ms；不要在 spec 里散落 30s 固定等待。

### 移动模拟器独立录屏

需要单独采集手机素材而不是运行完整双端传输时，用 `e2e/desktop/scripts/record-mobile-simulator.mjs`：

```bash
# Android Emulator，默认使用 emulator-5554；其他序列号通过 ANDROID_SERIAL 指定
ANDROID_SERIAL=emulator-5554 pnpm --dir e2e/desktop record:mobile android

# 自动录制 10 秒，便于快速验证或截取短素材
pnpm --dir e2e/desktop record:mobile android 10
```

**正确做法**：Android 使用 `adb shell screenrecord` 后自动 pull 到 `e2e/desktop/build/desktop-recordings/raw/`。iOS 的命令行录制依赖 `simctl`，当前本机图形服务不可用，改用 Simulator 的 `Cmd+R` 手动录制；完整结论见 [demo-recording.md](demo-recording.md)。移动端单独录制只负责画面；真实双端传输继续使用 `record:transfer`，由它统一驱动流程、录屏和收尾。

**相关文件**：`e2e/desktop/scripts/record-mobile-simulator.mjs`、`e2e/desktop/package.json`

**相关文件**：`e2e/desktop/scripts/record-transfer-demo.mjs`、`../SwarmDrop-RN/e2e/webdriver/`

## Cargo 配置

### dev profile 所有依赖 opt-level = 3

`Cargo.toml` 根：

```toml
[profile.dev.package."*"]
opt-level = 3
```

**Why**：crypto 依赖（`chacha20poly1305` / `blake3` / `sha2` / `ed25519` 等）和 libp2p 不开优化会慢 10-100×，dev 体感卡顿明显。

> 历史：这条最初是被 `tauri-plugin-stronghold` 逼出来的（它是当时最慢的一个）。
> **Stronghold 已移除**（私钥改由系统钥匙串 `keyring` 管理），但这段配置**必须保留**
> ——libp2p 与其余 crypto 依赖同样吃它。

**不要做**：删除这段配置或把 `*` 改成具体 crate 列表——会漏掉新加的 crypto/网络依赖。

### `webrtc-p2p` 的建连测试在 CI 上会偶发超时（本地稳定通过）

`crates/webrtc-p2p/tests/native_signaling.rs::established_connection_carries_data`
在 2026-08-10 的 v0.15.2 发版跑里以「15s 内应完成开流: Elapsed(())」失败，**重跑即过**。

判定它是 flaky 而非回归，用的是这三条（下次再遇到照同一套查，别急着改代码）：

1. 那一轮的 commit **零改动** `crates/webrtc-p2p`（`git log --name-only` 一搜就知道）；
2. 同一份代码本地 `cargo test --workspace` 该条通过；
3. `gh run list --workflow=rust.yml` 近 15 次只有这一次红。

成因是超时阈值对 CI 偏紧：这条测试要走完 SDP 交换 + ICE 收集 + DTLS 握手 + SCTP 建立，
而 GitHub 共享 runner 的 CPU 是抢占式的，本机的 15s 余量在那里不一定够。

**目前不改**——单次失败不足以判定为系统性 flaky，盲目放宽超时会掩盖真实的建连回归。
但**它若再出现**，就该动阈值或加重试，而不是继续手动重跑：一条「每次都要重跑一下」的
测试等于没有测试，因为没人分得清这次的红是 flaky 还是真的坏了。

### ⚠️ `sed -i.bak` 还原会带回**旧 mtime**，骗过 cargo 的增量编译（2026-08-11 踩到）

做「改坏 → 跑测试确认它会红 → 还原」这类**负向验证**时，常见写法是
`sed -i.bak 's/好/坏/' f.rs`，验证完 `mv f.rs.bak f.rs` 还原。

问题：`.bak` 保留的是**改动前的 mtime**，`mv` 回来后源文件比 target 里的编译产物还「旧」，
cargo 判定没变化，**直接复用「坏」那次的产物**。表现是：代码明明是对的、`grep` 也确认
了，测试却持续红——排查方向会全跑偏（我当时怀疑到了 crate 解析、feature 门控、`.bak`
是否还原错文件）。

**还原之后 `touch` 一下**，或者干脆用 `git checkout -- <file>`（它写的是新 mtime）。

负向验证本身**必须做**：本轮有一条测试第一版就是假的——断言写对了，但用满尺寸帧
复现不出要测的合并行为，改坏实现它照样绿。**「测试通过」不等于「测试有效」，只有让它
红过一次才知道。**

### 门禁顺序：`cargo test` 要排在 `cargo clippy --all-targets` **前面**

跑一轮 `cargo test --workspace` 的墙钟里，**真正执行测试的只占 7%**。2026-08-10 实测：
540 个测试全部跑完累计 **209 秒**，而墙钟是 **47 分钟**——另外 93% 全在 codegen 与链接。

原因是这仓的两条放大器叠在一起：`[profile.dev.package."*"]` 的 `opt-level = 3` 让整棵
依赖树（libp2p / webrtc-rs / tauri）都走优化编译，而 `--workspace` 有 **28 个测试二进制**，
每个都要单独链接一遍那棵树，链接还是单线程的。

**关键是 `check` / `clippy` 的产物 `test` 一个都用不上**：前两者只产 `rmeta`，测试要真
codegen 出 rlib 与可执行文件；clippy 还带 `RUSTC_WORKSPACE_WRAPPER`，fingerprint 又是
另一套。所以把 `clippy --all-targets` 排在 `test` 前面，等于让机器把同一批代码**编两遍
且不能复用**——那轮白搭了二十多分钟。反过来 `test` 先跑，clippy 能复用它的 codegen 产物。

顺带：这也是为什么「跑门禁时别再起第二条 cargo」。第二条会阻塞在 target 目录的文件锁上，
看起来像它自己卡住了，实际还把正在跑的那条一起拖慢。

### `target/` 是 10G 量级，跑全量测试要留够盘

`opt-level = 3` + libp2p / webrtc-rs / tauri 三棵大依赖树的合并后果：`cargo clean` 之后
跑一轮「check + 相关测试 + wasm 双 target + 桌面 bindings 导出 + mobile cdylib」就能重新
长到 **10–11G**。`cargo test --workspace` 更吃——它要为每个 test target 各编一个二进制。

盘紧时的取舍，按性价比排：

| 手段 | 腾出 | 代价 |
|---|---|---|
| `rm -rf target/*/debug/incremental target/debug/incremental` | 见下 | 只影响下次增量编译速度，**最安全** |
| `CARGO_INCREMENTAL=0` 跑 | 不再增长 | 单次编译略慢，适合一次性的门禁跑 |
| `cargo clean` | 全部 | 全量重编（libp2p 那棵树十几分钟） |

**「10G 量级」是 `cargo clean` 之后的一轮，不是长期稳态。** 2026-08-07 实测：长期开发
（多 target × 多 profile 累积）后 `target/` 到了 **176G**，其中 `debug/deps` 101G、
各 target 的 `incremental` 合计 39G、`wasm32-unknown-unknown` 11G，直接把 466G 的盘塞到
只剩 467Mi，`cargo test --workspace` 与 clippy 全线报 `No space left on device`
（症状是成片的 "could not compile ... due to 1 previous error"，**看着像代码问题**，
要往上翻才看得到真正的 `couldn't create a temp dir` / `failed to write`）。

删 incremental 那一条别只删 `target/debug/incremental`——iOS / Android / wasm 各 target
都有自己的一份，那次合计 39G 里 `debug` 只占 26G。一条命令覆盖全部：

```bash
rm -rf target/debug/incremental target/*/debug/incremental
```

**`cargo clean` 治标不治本**：清完的余量若也只有 10G 出头，编一轮就又满了。真卡住时从
项目外腾（Xcode DerivedData / Android 构建缓存都是纯派生物）比反复 clean 有效。

跑不完全量测试时的降级顺序：先 `cargo test -p <改动的 crate>`（多数改动只需要一两个），
完整 `--workspace` 交给 CI——它在 ubuntu runner 上跑，不受本机盘限制。

### wasm 构建：macOS 必须装 brew 的 LLVM，系统 clang 不行

**任何要编到 `wasm32-unknown-unknown` 的活都会撞这个**（当前是 `spike/iroh-web`，M2/M6 也躲不掉）。

**Apple 自带的 clang 阉割了 WebAssembly backend** —— `clang -print-targets` 里一条 wasm 都没有。
凡是依赖里有需要编 C 的 crate（如 `ring`，iroh 的 `tls-ring` feature 会拉进来），cc-rs 调系统
clang 必然失败：

```
error: unable to create target: 'No available targets are compatible with triple "wasm32-unknown-unknown"'
error occurred in cc-rs: ... clang ... ring-0.17.14/crypto/curve25519/curve25519.c
```

**正确做法**（`brew install llvm` 后，在该 crate 的 `.cargo/config.toml`）：

```toml
[target.wasm32-unknown-unknown]
# getrandom 0.3 在 wasm 上必须显式指定 backend，少了编不过且报错不指向这里
rustflags = ['--cfg', 'getrandom_backend="wasm_js"']

[env]
CC_wasm32_unknown_unknown = "/opt/homebrew/opt/llvm/bin/clang"
AR_wasm32_unknown_unknown = "/opt/homebrew/opt/llvm/bin/llvm-ar"
```

**注意**：这是 macOS 工具链的问题，不是 iroh/ring 的问题；Linux 的发行版 clang 通常自带 wasm
target，所以 CI 上不需要这段 —— 别因为「CI 能过」就以为本机不用配。

**wasm-pack 不必手动 pin `wasm-bindgen`**：它会从 `Cargo.lock` 解析出版本、自动装匹配的
`wasm-bindgen-cli`（见其 `src/lockfile.rs` 的 `require_wasm_bindgen`，实测装了 v0.2.126）。
iroh 官方 browser-echo 示例里那个 `wasm-bindgen = "=0.2.122"` 精确 pin 是**手工串链路**的产物
（`cargo build` 后自己调 `wasm-bindgen` CLI，两者 schema version 对不上直接报错），
用 wasm-pack 就不必背这个包袱。

**相关文件**：`spike/iroh-web/.cargo/config.toml`、`spike/iroh-web/README.md`

### `crates/web` 的 wasm 测试要跑起来，chromedriver 主版本必须与本机 Chrome 一致

`crates/web` 整个 crate 是 `#[cfg(wasm_browser)]`，**进不了 `cargo test --workspace`**；
而 `check-wasm.sh` 的 `--all-targets` 只把 `#[wasm_bindgen_test]` 模块**编**进来。
于是它那 20 条测试（IndexedDB 往返、OPFS、收件箱可见性与排序）长期处于
「写了、编得过、**从没跑过**」的状态——2026-08-03 首次执行才发现这一点。

代价是实打实的：`invite_store.rs` 的写读不对称（写 `serde_json::to_string` + `put_string`
存字符串，读却用 `serde_wasm_bindgen::from_value` 当对象解析，于是**每一行都被静默丢弃**，
已发出的邀请跨刷新全部消失）能活到被手动验证撞见——那条路径有往返测试覆盖，
只要执行过一次就会当场红。**编得过给的是虚假的安全感。**

**正确做法**：跑 `./scripts/test-wasm.sh`（CI 的 wasm job 里已接上）。它自己解析 Chrome
版本并取匹配的 chromedriver，不信任 PATH 里碰巧存在的那个。

**不要做**：直接 `wasm-pack test --headless --chrome crates/web` 而不管 driver 版本。
chromedriver 主版本与 Chrome 不一致时，失败长这样：

```
Starting new webdriver session...
Error: http status: 404
driver status: signal: 9 (SIGKILL)
```

那个 404 是 driver 拒绝了 wasm-bindgen runner 的 W3C 端点，**与「测试挂了」看起来毫无
区别**，极易误判成代码问题（Homebrew 的 chromedriver 跟着自己的节奏升级，与本机 Chrome
常年错位——实测 brew 装的是 151，系统 Chrome 是 150）。后面那个 SIGKILL 是 runner 自己
的清理，不是 Gatekeeper。

另外两个只在 macOS 出现的坑，脚本里都处理了：

- 下载来的 chromedriver 带 quarantine 且签名过不了 Gatekeeper，**启动即被 SIGKILL**。
  `xattr -dr com.apple.quarantine` + `codesign --force --sign -` 自签名可放行。
- wasm-pack 从 **PATH** 取 chromedriver，`CHROMEDRIVER` 环境变量会被它覆盖掉；且它内部
  `cd crates/web` 再跑 cargo，所以前置进 PATH 的目录**必须是绝对路径**——相对路径的症状是
  `No such file or directory (os error 2)`，同样与「driver 没装」无从区分。

#### ⚠️ driver 缓存在 `target/` 下，会被 rust-cache 掏空成骨架（2026-08-06 修）

**症状**：CI 的 `wasm` job 连红，`./scripts/test-wasm.sh` 报

```
All providers failed for chromedriver 150.0.7871.128:
  - DefaultProvider: The browser folder (target/wasm-test-driver/chromedriver/linux-150.0.7871.128)
    exists but the executable (…/chromedriver-linux64/chromedriver) is missing
```

**成因**：`DRIVER_ROOT` 在 `target/` 下，而 `swatinem/rust-cache` 会缓存并**清理** `target/`
——它不认识非 cargo 产物，于是恢复回来的是**只有目录、没有二进制**的骨架。而
`@puppeteer/browsers install` 见到版本目录已存在就拒绝安装，不会自愈。

净效果：**CI 上第一次跑完之后每一次都必然失败**，且报错长得像网络问题。develop 在
2026-08-06 连红三次都是它，期间那 25 条 wasm 测试一次都没在 CI 跑过——正是这个脚本当初
要消灭的「写了、编得过、从没跑过」，换了个地方复发。

**修法**：脚本发现「缓存里没有匹配当前 Chrome 版本的 driver」时，先 `rm -rf "$DRIVER_ROOT"`
再装。走到那一步就已经确定缓存无用，清掉无损；不做定点删除是为了不把 puppeteer 的目录
布局（`<root>/chromedriver/<platform>-<version>/…`，且 `<platform>` 在 Apple Silicon 上是
`mac_arm` 不是 `mac`）抄进脚本。

**复现方式**（照着做能精确重演，别用「随便造个空目录」——目录名不对就撞不上）：
先正常跑一次让它装好，再 `rm -f $(find target/wasm-test-driver -name chromedriver -type f)`，
然后重跑。修复前报上面那条，修复后自愈并 25 passed。

**相关文件**：`scripts/test-wasm.sh`、`scripts/check-wasm.sh`、`.github/workflows/rust.yml`

### docs 的 Next dev：浏览器必须用 localhost 访问，127.0.0.1 会静默死页

Next.js dev server（`cd docs && pnpm dev`）以 `localhost:3000` 起，浏览器若用
`http://127.0.0.1:3000` 打开，dev 资源（webpack-hmr、字体、client chunk）会被
**Blocked cross-origin request** 拦截——症状极具迷惑性：页面正常渲染（SSR HTML），
但**没有 hydrate**，所有按钮点了没反应、console 无任何报错，看起来像业务代码坏了。
服务端日志（pnpm dev 的输出）里才有 Blocked 警告。

**正确做法**：`/app` 等交互页实测一律 `http://localhost:3000`；或在 `next.config.mjs`
加 `allowedDevOrigins: ['127.0.0.1']`。（README 里"实测用 127.0.0.1"说的是**静态
serve 的产物**，与 Next dev 是两回事。）

**相关文件**：`docs/next.config.mjs`、`docs/app/app/`

### spike/ 不进 workspace

`spike/` 放临时的技术验证（当前：`spike/iroh-web`，见 #60），根 `Cargo.toml` 里
`exclude = ["spike"]`。

**Why**：
- spike 通常是 **wasm-only / 平台专用**的，进 members 会被 `cargo check --workspace` 用桌面
  target 白编一遍，纯浪费
- spike 自带的 `[profile.release]` 进了 workspace 会被 root **静默忽略**（同 mobile-core 并入
  时踩的那个坑）
- 不 exclude 的话 cargo 会报「在 workspace 目录内却不是 member」

**不要把 spike 放 `crates/`** —— 那是生产位置（如 `crates/web` 是 #72 定的），spike 可能失败，
要能整目录删掉不留痕。验证通过后再按架构文档挪到正式位置。

**wasm crate 转正到 `crates/` 时会撞上 profile 限制**（spike 期靠 exclude 绕过，转正就绕不掉了）：
Cargo 的 `[profile.*]` **只能在 workspace root 生效**，成员 crate 的 profile 被静默忽略（同
mobile-core 并入时那个坑）。给单个 crate 定制 profile 的**唯一**办法是该 crate 自己的
`.cargo/config.toml` —— iroh 官方 browser-chat 就是这么做的（`browser-wasm/.cargo/config.toml`，
注释：*"we specify the profile here, because it is the only way to define different settings for a
single crate in a workspace"*）。代价是从 workspace root 构建时这份 profile 不生效。

体积影响不小：官方 browser-blobs 缺 `[profile.release]` 那 6 行（`opt-level="z"` / `lto` /
`codegen-units=1` / `panic=abort` / `strip="symbols"`），白白多付约 **39%** 的 gzip 体积。

### workspace members 固定 5 个（含移动端桥接 crate）

```toml
members = [
    "crates/core", "crates/entity", "crates/migration", "src-tauri",
    "mobile/packages/swarmdrop-core/rust/mobile-core",
]
```

加新 crate 必须显式登记，否则 `cargo check --workspace` 不会覆盖——移动端并入单仓的
核心价值正是让 `cargo check --workspace` 覆盖 mobile-core，漏登记等于白并。

## Vite / Tauri 联动

### 端口固定 1420，HMR 走 1421

Tauri dev 期间硬编码连这两个端口。改 `vite.config.ts` 端口会让 `pnpm tauri dev` 白屏。

**相关文件**：`vite.config.ts`

### Windows 开发时必须忽略根目录 `target/`

Cargo workspace 的构建产物位于仓库根目录 `target/`。若 Vite 监听到 Cargo 正在写入的 `.exe`，Windows 会报 `EBUSY`，并使 `beforeDevCommand` 退出，进而导致 `pnpm tauri dev` 失败。

**正确做法**：
- 在 `vite.config.ts` 的 `server.watch.ignored` 中同时保留 `"**/src-tauri/**"` 和 `"**/target/**"`。

**不要做**：
- 只忽略 `src-tauri/**`；它不包含根目录 `target/`。

**相关文件**：`vite.config.ts`

### TAURI_DEV_HOST 用于真机调试

`vite.config.ts` 读取 `TAURI_DEV_HOST` env：设了就把 host / hmr.host 切到该 IP。本地 dev 不需要设。

### /impeccable live 模式对 Tauri 原生窗口可行，别一上来就判"不支持"

第一反应容易觉得 live 模式（`.claude/skills/impeccable`）只认浏览器 tab，Tauri 原生窗口用不了——**这个判断是错的**，已经实测验证过。

**为什么能用**：live 模式的注入机制就是往 `index.html`（见 `.impeccable/live/config.json` 的 `files`）插一段 `<script>`，而 `pnpm tauri dev` 本质是 `BeforeDevCommand: pnpm dev --host` 起 Vite，Tauri 原生窗口只是加载同一个 Vite dev server 的 `index.html`。跑 `node .claude/skills/impeccable/scripts/live.mjs` 注入后，Vite 的 HMR 会让已经打开的 Tauri 窗口自动重载，注入的悬浮选取器工具条会直接出现在真实窗口里，App 本身也照常渲染（因为 Tauri IPC 上下文还在）。

**踩过的坑**：单独用 `pnpm dev`（不走 `pnpm tauri dev`）在普通浏览器 tab 里打开 `http://localhost:1420` 会是**空白页**——这是因为前端 mount 时就会走 Tauri IPC（`commands.initializeIdentity()` / network-store 等），普通 Chrome tab 没有 `window.__TAURI_INTERNALS__`，直接崩渲染。所以"浏览器 tab 打开空白"和"live 注入机制在 Tauri 里失效"是两件不同的事，别混为一谈。

> 注意：这里说的桌面前端（`src/`）与 **Web 端**（`crates/web` + `docs/app/app`）是两套东西。
> Web 端不走 Tauri IPC，本来就在浏览器里跑。

**agent 这边怎么驱动**：live.md 文档写的是用 `browser_navigate` 之类的浏览器工具去看/截图，这对 Tauri 不适用；改用 `mcp__tauri__driver_session`（需要项目已装 `tauri-plugin-mcp-bridge`，本仓库已装）连接同一个正在跑的原生窗口，`mcp__tauri__webview_screenshot` / `webview_execute_js` 代替浏览器截图/取值，`live-poll.mjs` 的本地 HTTP helper 完全不关心注入的 JS 跑在哪个 webview 里，所以轮询/accept/discard 那套照常工作。

**已知脆弱点**：往 `index.html` 里改动（`live.mjs` 注入、或后续手动 edit 触发 HMR 全量刷新）会让已连接的 `mcp__tauri__` bridge 掉线——`webview_execute_js` / `webview_screenshot` 会报 `WebView execution failed`，需要重新 `driver_session action:"start"`；遇到过一次重连失败，发现是 `pnpm tauri dev` 整个进程都退出了（不是 panic，像是窗口被动关闭），得重新 `pnpm tauri dev` 再连。改动 index.html 或触发大范围 HMR 刷新后，预期要重连一次 driver_session，不代表出错。

**相关文件**：`.impeccable/live/config.json`（`files: ["index.html"]`）、`src-tauri/Cargo.toml`（`tauri-plugin-mcp-bridge`）

### Tauri MCP 事件模拟：payload 必须是对象，快捷发送链路可整段模拟

用 tauri MCP 验证 UI 时两个实测经验：

**正确做法**：
- 模拟「右键用 SwarmDrop 发送」全链路不需要真实右键：emit `external-file-open` 事件（payload `{ paths: ["/abs/path", ...] }`）即可触发 ExternalOpenHandler → share-store → `/send/share-target` 完整前端链路
- emit 事件时 payload 必须是 **JSON 对象**。`mcp__tauri__ipc_emit_event` 的 payload 参数如果传了字符串化 JSON，前端 `event.payload` 收到的是 string，`payload.paths` 为 undefined，listener 静默失败、页面毫无反应。保险做法是用 `webview_execute_js` 执行 `window.__TAURI__.event.emit("external-file-open", { paths })`

**不要做**：
- 通过 `window.location.href = "/xxx"` 验证路由 redirect——整页刷新会丢掉所有运行时 store
  （share-store 的待发送文件、network-store 的节点状态等），链路验证到一半就断了。
  用路由跳转 API，不要整页刷新。

> 该条最初写的是「会弹回 unlock 屏」。**密码/解锁流程已整体移除**（首启只问设备名，
> 身份由后端 keychain 静默管理），但「整页刷新丢内存态」这个坑本身依然成立。

**相关文件**：`src/components/external-open-handler.tsx`、`src/lib/bindings.ts`（`events.externalFileOpen`）

## Git submodule

### 本仓已无 submodule（libs/ 于 2026-07 删除）

**曾经**：`libs/` 是 `swarm-apps/swarm-p2p` 的 submodule，提供 `swarm-p2p-core`，克隆后必须
`git submodule update --init --recursive` 才能 `cargo build`。

**现在**：网络栈由自研的 `crates/net` + `crates/net-base` 取代，`libs/` 已从工作树删除，
`.gitmodules` 不存在。**克隆后直接 `pnpm install` 即可**，无需任何 submodule 步骤。
swarm-p2p 的历史源仍在独立仓 `swarm-apps/swarm-p2p`，但本仓不再依赖它。

- workspace member 全在 `crates/*` + `src-tauri` + `mobile-core`
- `.github/workflows/rust.yml` 各 job 里的 `submodules: recursive` 现为 no-op，
  保留只作未来再引入 submodule 的兜底
- 迁移背景见 [net-kernel.md](net-kernel.md)，决策见 `dev-notes/why-libp2p-not-iroh.md`

## Lingui 提取

### sourceLocale 是 zh，实际只有 3 个 locale

```ts
sourceLocale: "zh",
locales: ["zh", "zh-TW", "en"],
```

ja/ko/es/fr/de 只是规划目标，**当前实际是 3 个**。新增 locale 前先确认设计资源就绪。

补翻译时只需覆盖这 3 个；`src/locales/` 下没有其它语言目录，不要为尚未落地的 locale 建空目录。

### 提取命令必须先于 commit

```bash
pnpm i18n:extract
```

漏跑会导致 `src/locales/*/messages.po` 缺少新加的字符串，运行时降级显示原文。

### ⚠️ Web 应用区漏跑不会「降级显示原文」，会直接显示 msgid（2026-08-05 踩到）

桌面（Vite + babel macro）漏跑时确实降级成源文；**Web 应用区不是**。`docs` 的 `pnpm build`
第一步就是 `lingui compile --typescript`，编出来的 catalog 里查不到某条 id 时，界面上出现的是
**生成的 msgid 本身**——像 `YfX8tg` 这样一串六位随机字符，看起来像个会话号或错误码，
完全不像缺翻译。

更阴的是**只有生产构建会暴露**：dev 下同一句话显示正常，所以「改完在 dev 里看了一眼没问题」
挡不住它。

触发条件比想象中低——**改标点就够了**。把「现在没有正在进行的传输。」的句号去掉，
Lingui 眼里就是一条全新的 msgid，旧译文自动进 `#~` 废弃区，en / zh-TW 当场缺两条。

所以：**动了任何 `<Trans>` / `` t`` `` 里的字符（哪怕只是标点），必须**

```bash
cd docs && pnpm i18n:extract     # 看 Missing 那一列
# 补完 en / zh-TW 的 msgstr 后再跑一次，Missing 必须归零
cd docs && pnpm i18n:extract
```

改标点时旧译文就在同一个文件的 `#~ msgid` 里，照抄改标点即可，不用重译。

## dev server 与 `next build` 抢 `.next/`，产物 CSS 会是旧的（2026-08-05 实证）

`next dev` 与 `next build` **共用 `docs/.next/`**。dev server 开着跑 `pnpm build`，构建会
「成功」并写出完整的 `out/`，但里面的 CSS 可能是**改动之前的**——没有任何警告。

实证：改了 `app/global.css` 的十来处（新 token + 新组件类），`pnpm build` 后产物里
`--app-shell-background` 还是旧值 `#fbfcfc`，`--space-section` / `--glass-rail-bg` /
`--scrollbar-thumb` 一条都没有，而改动前就存在的 `--radius-panel:24px` 在。
**停掉 dev server + `rm -rf .next out` 后重建，全部正确。**

危险的不是构建失败，是构建**看起来成功**：验证「我的 CSS 改动进产物了吗」时会得到一个
假阴性（以为没生效，去改代码），而 CI 里没有 dev server、构建是对的，于是本地与线上表现不一致。

**所以：验证产物前先停 dev server。** 判据（grep 一个只在新代码里出现的 token 即可）：

```bash
cd docs && CSS=$(find out -name "*.css")
grep -o -- "--你新加的-token:[^;}]*" $CSS   # 空 = 产物是旧的
```

这也是「一次只起一个 server」那条纪律的具体成因之一。要看真实形态用 `pnpm start`
（`serve out`，只读静态产物，不碰 `.next`），别再开一个 dev。

## 测试

### mobile-core 的文件测试用**固定** temp 路径，别并发跑两次 `cargo test --workspace`

`file_staging` 与 `file_access` 的 6 个测试各自写死了 `std::env::temp_dir().join("swarmdrop_staging_truncate")`
这类固定目录名。同时跑两个 `cargo test --workspace`（比如一个丢后台、又在前台跑一次）会让
两边抢同一个目录，表现是这 4 条稳定失败：

```
file_staging::tests::discard_removes_the_file
file_staging::tests::truncate_clears_while_reopen_preserves
file_access::tests::publish_to_local_renames_within_the_same_volume
file_access::tests::publish_to_local_rejects_escape_through_symlink
```

**判据**：单独 `cargo test -p swarmdrop-mobile-core` 全过 = 是并发干扰，不是回归。2026-08-09
就这么误判过一次。真要并发验证，先确认没有另一个 cargo test 在跑。

（治本是给这些路径加唯一后缀，属于既有缺陷，未在本轮处理。）

**相关文件**：`mobile/packages/swarmdrop-core/rust/mobile-core/src/file_staging.rs`、
`mobile/packages/swarmdrop-core/rust/mobile-core/src/file_access.rs`

## 版本号同步：两条独立版本线

单仓但**两条版本线**，各自打 tag、各自发版，互不干扰：

| | 桌面 | 移动 |
|---|---|---|
| tag | `v0.8.0` | `mobile-v0.7.19` |
| workflow | `.github/workflows/release.yml` | `.github/workflows/mobile-release.yml` |
| SwarmHive app | `swarmdrop` | `swarmdrop-rn` |
| 版本真源 | `src-tauri/tauri.conf.json` | `mobile/app.json` 的 `expo.version` |
| 跟随项 | `package.json`、`src-tauri/Cargo.toml` | `mobile/package.json` |

同一条线内的几处必须一起改（已经踩过坑）。CI 的 `verify-versions` job 会在构建前校验
真源、跟随项、tag 三者一致，不一致直接 fail——不必再靠人记。

**为什么不统一版本线**：移动端历史版本号已到 0.7.18、高于桌面 0.7.8。统一到任何
`< 0.7.18` 的版本对移动端都是**降版**，存量用户收不到更新。

**tag 互斥靠 glob 从头匹配**：`v*` 不会匹配 `mobile-v0.7.19`。别把移动端 tag 起成
`v-mobile-*` 之类以 `v` 开头的形式，会同时点燃两条流水线。

`crates/core/Cargo.toml` 的 version 与两条线都无关，它是共享 core 自己的版本。

## 跑 `crates/web` 的 wasm 测试：别用 `wasm-pack test`

`wasm-pack test --headless --chrome` 有两个坑，叠在一起时报出来的错完全指不到病因：

1. **它吃路径不吃 `-p`**。`wasm-pack test … -p swarmdrop-web` 会去解析仓库根的 `Cargo.toml`，
   报 `failed to parse manifest: missing field package`（workspace 根没有 `[package]`）。
   正确形式是 `wasm-pack test … crates/web`。
2. **它强制用自己缓存的 ChromeDriver，且会覆盖你传的 `CHROMEDRIVER` 环境变量**。
   缓存里那个 driver 的大版本与本机 Chrome 对不上时（写这条时是 driver 151 / Chrome 150），
   driver 起来即被 SIGKILL，而 runner 报的是 **`Error: http status: 404`** ——
   看着像网络或 URL 问题，其实是版本不匹配。别去 `codesign`，签名不是病因。

**绕开 wasm-pack 直驱 cargo**，两个坑都不存在：

```bash
cd crates/web
CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER="$HOME/Library/Caches/.wasm-pack/wasm-bindgen-cargo-install-<ver>/wasm-bindgen-test-runner" \
CHROMEDRIVER=<与本机 Chrome 同大版本的 chromedriver> \
WASM_BINDGEN_TEST_ONLY_WEB=1 \
cargo test --target wasm32-unknown-unknown
```

chromedriver 按 Chrome 大版本从 <https://googlechromelabs.github.io/chrome-for-testing/> 取。

**这些测试不在任何 CI 门禁里**：整个 `crates/web` 是 `#[cfg(wasm_browser)]`，
进不了 `cargo test --workspace`；而 CI 也没有 headless Chrome。
`scripts/check-wasm.sh` 只保证它们**编得过**（脚本末尾对 `swarmdrop-web` 单独加了一轮
`--all-targets`，其余 crate 不能加 —— 它们的 dev-dependencies 里有 tokio/mio 这类
native-only 的东西，wasm target 下直接编不过）。所以改 `crates/web` 的逻辑后，
**编过 ≠ 测过**，要真跑一遍得用上面那条命令。

## 三份自动生成的 bindings 都会静默漂移 —— 没有任何门禁拦它

本仓有三份「由 Rust 生成的 TS」，**没有一份在 CI 里被校验**，于是它们会长期落后于源码
而不报错：

| 产物 | 谁生成 | 什么时候生成 |
|---|---|---|
| `src/lib/bindings.ts` | tauri-specta | `pnpm tauri dev`（debug 启动时）或 `cargo test -p swarmdrop export_ts_bindings` |
| `crates/web/bindings/bindings.ts` | specta | `cargo test -p swarmdrop-web --features specta --test specta_export` |（**漏了 `--features specta` 会静默什么都不做**，见下）
| `mobile/packages/swarmdrop-core/src/generated/` | uniffi (ubrn) | `ubrn build ios/android --and-generate`（要 Xcode / NDK） |

2026-08-01 那次简化里，三份**同时**被发现落后于已提交的 Rust：桌面那份还带着已删的
`pauseTransfer`、`start` 的参数个数也不对；uniffi 那份根本没有 `renameDevice` /
`PairedDeviceRemoved` / `DeviceRenamed`，`startNode` 还带着已删的 `deviceName` 参数。
前端能跑只是因为它调用的恰好是交集。**按移动端的失败模式，落后的 uniffi 绑定进 CI 会打出
「启动即 checksum mismatch」的包。**

**做法**：改了跨 IPC/FFI 边界的类型或命令签名，当场重生成对应那份并一起提交。不要指望
「下次 `tauri dev` 会自动更新」——那只在有人恰好跑 dev 的时候才发生，而 CI 从不跑 dev。

### Web 那条：`cargo test -p swarmdrop-web` 漏了 `--features specta` 会**装作成功**

导出测试整个文件是 `#![cfg(all(not(target_family = "wasm"), feature = "specta"))]`，所以不带
feature 时它不是失败、也不是报错，而是被编译掉后显示：

```
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out
```

`ok.` + 退出码 0，唯一的线索是那个 **`1 filtered out`**。2026-08-10 就是这么白等了一轮：
桌面那份已经生成好了，Web 这份的命令跑完也报 ok，直到 `grep FilePublish` 零命中才发现。
**判据不是退出码，是产物里有没有你刚加的类型**——重生成之后 grep 一下再说它好了。

### 没有 Xcode / NDK 时怎么重生成 uniffi 绑定

`ubrn build` 要原生工具链，但只重生成绑定不需要：

```bash
cargo build -p swarmdrop-mobile-core       # 先出 target/debug/libswarmdrop_mobile_core.dylib
cd mobile/packages/swarmdrop-core/rust/mobile-core   # ⚠️ 必须 cd 到 Cargo.toml 所在目录
../../../../node_modules/.bin/ubrn generate jsi bindings \
  --library ../../../../../target/debug/libswarmdrop_mobile_core.dylib \
  --ts-dir ../../src/generated --cpp-dir ../../cpp/generated
cd ../.. && ../../node_modules/.bin/bob build   # ⚠️ 不能省
```

三个易踩点：cwd 不在 crate 目录会报 `manifest not exist`；**`bob build` 不能省**——
`pnpm typecheck` 与 Metro 看的都是 `lib/typescript/src/generated/*.d.ts`，只重生成
`src/generated` 的话 tsc 仍报旧类型；环境没有 prettier/clang-format 不影响 diff
（仓库现存产物本来就是未格式化的）。跑两遍产物字节一致，是确定性的。

**相关文件**：`src-tauri/src/setup.rs:293`（`export_ts_bindings` 测试）、
`crates/web/tests/specta_export.rs`、`mobile/packages/swarmdrop-core/ubrn.config.yaml`、
`mobile/dev-notes/knowledge/rust-bridge.md`

## OpenSpec：两条只有踩过才知道的规则（2026-08-19 实证）

### 未归档 capability 的 `MODIFIED` delta 能过 `validate`，但会卡在 `archive`

`openspec validate` 只检查 delta 自身的格式（有 `## MODIFIED Requirements` 头、每条
Requirement 至少一个 `#### Scenario`），**不检查被修改的 capability 是否存在于
`openspec/specs/`**。所以对一个仍在别的 change 里、尚未归档的 capability 声明 MODIFIED，
会得到一句干脆的 `Change 'xxx' is valid`——而它在归档时无处可合并。

判据：写 delta 前先 `ls openspec/specs/<capability>`。不在那里就意味着**从 OpenSpec 的
视角看这个能力还不存在**，此时正确的做法通常是**就地更新那个未归档 change 的 spec**
（它还是草稿），而不是造一个永远合并不进去的 delta。

代价要记：就地更新没有 `openspec validate` 兜底，必须落成显式任务，否则漏掉的那处会让
规格描述一个已经不存在的形态。

### 规格里不要写具体命令名 / 函数名

`standalone-cli-host` 的 `cli-host` spec 通篇写「设备列表命令」「配对生成」这类**角色
描述**，唯一的字面量是 `start`。于是 2026-08-19 把整个 CLI 命令面从 `pair`/`devices`
重整成 `invite create`/`device list` 时，那份 17 条 Requirement 的规格**只有一句话需要改**
（而且那一句改的是行为不是命名：「此时节点是新起的临时节点」这个括号说明，在设备列表
不再起节点之后失效）。

反过来说：如果规格里写满了命令名，一次改名就会变成一次规格重写，而重写里混进行为变更
是看不出来的。规格描述**契约**，命令名属于实现。

## `swarmdrop` 这个 bin 名归 CLI 独占，桌面壳的 bin 是 `swarmdrop-desktop`（2026-08-19 修复）

**现状**：`src-tauri/Cargo.toml` 有显式 `[[bin]] name = "swarmdrop-desktop"`。
**对外产物名一个都没变**（`swarmdrop.app` / `swarmdrop_0.23.0_aarch64.dmg` /
`swarmdrop.app.tar.gz` —— 它们由 `productName` 决定，与 bin 名无关）；变的是
`.app` 内部的 `Contents/MacOS/swarmdrop-desktop`（`CFBundleExecutable` 由 Tauri 一并写对）、
Windows 的 `swarmdrop-desktop.exe`，以及未打包的 cargo 产物。

下面记的是修之前的形态——因为这类冲突极易在「给某个 crate 加个 bin」时再次引入，
而它的症状与根因离得非常远。

---

修之前，`crates/cli` 的 `[[bin]] name = "swarmdrop"` 与 `src-tauri` 的 package name
`swarmdrop`（默认 bin 名 = package 名）**输出到同一个 `target/debug/swarmdrop`**。
Cargo 自己会警告：

```
warning: output filename collision at target/debug/swarmdrop
  = note: the bin target `swarmdrop` in package `swarmdrop-cli` has the same output
          filename as the bin target `swarmdrop` in package `swarmdrop`
  = note: this may become a hard error in the future; see rust-lang/cargo#6313
```

**后果一：CLI 的集成测试随机失败。** `crates/cli/tests/without_a_node.rs` 经
`env!("CARGO_BIN_EXE_swarmdrop")` 跑二进制，而那个环境变量指向的就是被覆盖的路径。
最后构建的那个赢：桌面壳赢的时候，6 条测试里有 4 条以「退出码 0、stdout 空」失败
（桌面应用在无窗口环境下启动即退出），报出来的却是
`["pair"] 仍然可用——旧命令必须彻底消失` 这类**完全指错方向**的断言消息。

判别方法是看文件大小：CLI 约 83 MB，桌面壳约 148 MB。
`cargo test -p swarmdrop-cli` 单跑必过（只构建 CLI 那个），所以这条只在 workspace 全跑时现形
——**包括 CI 的 `rust.yml`**。

**后果二：仓库里会凭空多出一个 77 KB 的 `crates/src/lib/bindings.ts`。** 桌面壳在 debug 下
启动时会自动导出 specta bindings，而那处用的是**相对 cwd** 的
`"../src/lib/bindings.ts"`（`setup.rs:162`；`setup.rs:341` 那处用
`concat!(env!("CARGO_MANIFEST_DIR"), …)` 是绝对的，没这个问题）。集成测试的 cwd 是
`crates/cli/`，于是被误跑的桌面壳把 bindings 写到了 `crates/cli/../src/lib/bindings.ts`。
它不在 `.gitignore` 里，会直接出现在 `git status` 的未追踪列表里。

**后果三：cargo 说了这将来是硬错误。** 到那天整个 workspace 构建失败。

**修法**：给 `src-tauri` 的 bin 显式改名，用 `tauri.conf.json` 的 `mainBinaryName`
把打包产物名改回来。Tauri 官方文档正是这么建议的——它明确写着 `mainBinaryName`
**不用来选择 bin target**，只在 `tauri build` 时重命名产物，并推荐「改 package name
或设 bin target 的 name field」来解决冲突本身。

```toml
# src-tauri/Cargo.toml
[[bin]]
name = "swarmdrop-desktop"
path = "src/main.rs"
```

### ⚠️ 别用 `mainBinaryName` 把产物名改回 `swarmdrop`（2026-08-19 实测的弯路）

直觉上 `tauri.conf.json` 的 `"mainBinaryName": "swarmdrop"` 正好能保住 `.app` 内部的
可执行文件名，两全其美。**实测下来它把冲突原样带了回来**：那个字段的作用是在
`tauri build` 阶段**把 cargo 产物重命名**（官方措辞是 "rename that binary in tauri-cli's
`tauri build` command"），也就是把 `target/debug/swarmdrop-desktop` **move 成**
`target/debug/swarmdrop`——于是 `swarmdrop-desktop` 消失、`swarmdrop` 又变成桌面壳。

`--no-bundle` 也一样重命名（那步在 bundle 之前）。**这比原来的形态更糟**：产物叫什么
取决于最后跑的是 `cargo build` 还是 `tauri build`，`e2e/desktop/wdio.conf.ts` 那种写死
路径的地方于是没有一个正确答案。

代价是 `.app` 内部与 Windows 的可执行文件名变成 `swarmdrop-desktop`。已核实无碍：
`external_open.rs` 的两处 `current_exe()` 只取**路径**去注册（Windows 注册表右键菜单 /
Linux `.desktop`），且带幂等比对，升级后首次启动会自动重写；single-instance 与 deep-link
按 bundle identifier 注册，都不看可执行文件名；`release.yml` 用 glob 交给
swarmhive-action 自己挑 updater bundle，也不硬编码。

**不能反过来改 CLI 的 bin 名**：用户敲的就是 `swarmdrop`，而且它已经作为
`cli/swarmdrop-cli-v*` 发到 npm 与 homebrew。

**跟着改的引用点**：`e2e/desktop/wdio.conf.ts` 的 `APP_BINARY_PATH`、
`.github/ISSUE_TEMPLATE/bug_report.yml` 里给用户的三条日志启动路径。

**验证**（2026-08-19 实跑）：`cargo build --workspace --bins` 无 collision 警告且两个
二进制并存（CLI 83 MB / 桌面壳 148 MB）；`cargo test --workspace` **837 条全过**
（此前它在 CLI 集成测试那里红 4 条）；`pnpm tauri build --debug` 产出
`swarmdrop.app` + `swarmdrop_0.23.0_aarch64.dmg` + `swarmdrop.app.tar.gz`（**名字与改动前
逐字相同**），内部是 `Contents/MacOS/swarmdrop-desktop` 且 `CFBundleExecutable` 一致。

## 提交前 checklist

```bash
pnpm exec tsc --noEmit
cargo check --workspace
cargo fmt --all
cargo clippy --workspace -- -D warnings   # 项目期望零 warning
./scripts/check-wasm.sh                   # wasm 双 target；改了 crates/* 必跑
```

改了跨边界类型还要**重生成三份 bindings 之一**（见上一节），CI 不会替你发现。

### SwarmdropCore.podspec 的 ubrn 版本号会漂移，症状伪装成 C++ 编译错（2026-08-07 踩到）

`mobile/ios` 构建报：

```
no member named 'string_from_buffer' in 'uniffi_jsi::Bridging<std::string>'
  → packages/swarmdrop-core/cpp/generated/swarmdrop_mobile_core.cpp
```

**看着像生成的 C++ 有 bug，其实是 pod 版本对不上。** 完整链条：

1. `SwarmdropCore.podspec` 曾写死 `s.dependency "uniffi-bindgen-react-native", "0.31.0-2"`；
2. 该文件在 `ubrn.config.yaml` 的 **`noOverwrite`** 名单里 —— `ubrn build --and-generate`
   刻意不覆盖它，所以升级 npm 包时这行**不会**被同步；
3. `package.json` 升到 `0.31.0-3` 后，生成器按新版产出 C++，调用了新接口；
4. 而 podspec 的 `s.source` 是 **git tag**（不是本地 node_modules），CocoaPods 按
   `0.31.0-2` 从 git 拉了**旧运行时**，其 `UniffiString.h` 只有 64 行、没有那个成员；
5. 于是在 xcodebuild 阶段才炸，且错误指向生成的 .cpp，与真正的原因隔着两层。

**判据**：直接比两份头文件，一眼就能定位，不必读 C++。

```bash
diff mobile/node_modules/uniffi-bindgen-react-native/cpp/includes/UniffiString.h \
     mobile/ios/Pods/uniffi-bindgen-react-native/cpp/includes/UniffiString.h
```

**已修**：podspec 改成从 `package.json` 读，消除第二事实源——

```ruby
s.dependency "uniffi-bindgen-react-native", package["devDependencies"]["uniffi-bindgen-react-native"]
```

**若仍不同步**（Podfile.lock 锁着旧版时 `pod install` 不会升）：

```bash
cd mobile/ios && pod update uniffi-bindgen-react-native
```

⚠️ `pod install` 不够，`pod cache clean` 也不够 —— lock 与 podspec 的约束才是锁，
两者都指向新版之后 `pod update` 才会真正换掉运行时。

### 日志在哪、怎么取（2026-08-07 落地）

排查问题时先想「日志能不能拿到」。三端现状：

| 端 | 落盘位置 | 开发者怎么看 | 用户怎么交出来 |
|---|---|---|---|
| 移动 | app sandbox 的 `cache/logs/swarmdrop.<date>.log` | `adb logcat -s SwarmDrop` / Console.app（subsystem `com.yexiyue.swarmdrop`） | 设置 → 关于 → 诊断 → 导出日志（系统分享面板） |
| 桌面 | `app_log_dir()`（macOS `~/Library/Logs/com.yexiyue.swarmdrop`） | 终端启动看 stdout | 设置 → 关于 → 打开日志文件夹 |
| 浏览器 | — | 设置页事件日志面板 | 同左 |

按天轮转、保留 7 份；**文件层固定 `INFO`，控制台/平台层跟随 `EnvFilter`**
（默认 `swarmdrop=debug,swarmdrop_net=debug`）。`tracing-appender` 只能按时间轮转，
控量手段是「少写」而不是勤轮转，那条 `FILE_LEVEL` 常量就是控量点。

四条实施时踩到或差点踩到的：

1. **`non_blocking()` 的 guard 必须与应用同生命周期**。它一 drop，后台写线程就停、
   日志静默消失且**不报任何错**，没有反馈回路。移动端存进进程级 `OnceLock`，
   桌面端存进 Tauri managed state，两端各有一条测试钉住。
2. **移动端的目录参数是 `file://` URI**。expo 的 `Paths.*.uri` 带 scheme，
   `Path::new("file:///x")` 会得到一个名为 `file:` 的**相对**目录——写进去的东西下次
   读不回来。统一在 `mobile-core/src/utils.rs` 的 `parse_host_dir()` 边界解析一次，
   反向由 `to_host_uri()` 出去，内部一律 `PathBuf`。**别在新的消费点再剥一遍前缀。**
3. **日志系统自己要写第一行**。否则用户「什么都没做就导出」会拿到 0 字节文件，
   看起来像功能坏了。`init_logging` 成功后立刻写一条「日志已启动」并记下文件位置。
4. **Android 的平台层是自实现的**（`logging/android.rs`），因为现成 crate 全停更了。
   改它必须同时改那 6 条测试——级别映射、内嵌 NUL 替换、UTF-8 边界截断都没有反馈回路，
   错了只是日志变形或丢失，不会报错。

桌面端的文件层是**延迟装载**的：`init_tracing()` 在 `tauri::Builder` 之前跑（那时
`app_log_dir()` 还拿不到），先注册控制台层 + 一个 `reload` 空位，setup hook 里再把文件层
装进去。空位类型必须是 `Option<Box<dyn Layer<..>>>`，写成具体的 `fmt::Layer<..>` 会把
writer 类型烤进签名、装载时对不上。

**选型推导见** [`dev-notes/research/2026-08-logging.md`](../research/2026-08-logging.md)。

## CI / Release

单仓两条 release 流水线，各由自己的 tag 触发（见上「版本号同步：两条独立版本线」）。

### Bootstrap 的三条发布线与多架构镜像

`swarm-bootstrap` 是服务端基础设施，不能复用桌面 `v*` 或移动 `mobile-v*` 的发布触发器；
它使用第三条独立 tag：`bootstrap-vX.Y.Z`。工作流同时发布 GitHub Release 二进制和 GHCR 镜像。

**正确做法**：
- 在 `ubuntu-latest` 与 `ubuntu-24.04-arm` 原生构建 Linux amd64 / arm64；不要在单个
  x86 runner 上用 QEMU 构建 release Rust 二进制。
- 两个镜像构建必须是显式 job，`image-merge` 同时 `needs` 它们；不要用 matrix job 作为
  唯一依赖，GitHub 可能在一个 matrix 子任务完成时就调度合并，错误发布单架构 manifest。
- GHCR 两腿都只 push digest，最后用 `docker buildx imagetools create` 合成 manifest list；
  不能让两腿并发推同一个 tag。
- 延续上游 `ghcr.io/swarm-apps/swarm-bootstrap` 时，同时保留 `bootstrap-vX.Y.Z` 及其
  `-amd64` / `-arm64` 兼容标签，并增加 `X.Y.Z`、`X.Y`、`latest`。该包最初关联
  `swarm-apps/swarm-p2p`，首次由 `swarm-apps/SwarmDrop` 发版前须在包设置中授予其 Actions
  写权限；工作流优先使用可选的 `GHCR_TOKEN`，否则回退 `GITHUB_TOKEN`。
- 先创建 GitHub Release，再由两个 binary job 只追加 `.tar.gz` 与 `.sha256`；两个 job
  同时创建 release 会发生竞态并可能遗漏一个架构的文件。
- 运行前校验 tag 去掉 `bootstrap-v` 后与 `crates/bootstrap/Cargo.toml` 版本相等。

**相关文件**：`.github/workflows/bootstrap-release.yml`、`crates/bootstrap/{Cargo.toml,README.md}`
发版 = bump 该线的版本 + commit + tag + push tag。

### changelog 必须按路径 + tag 分流

移动端并入时带进 129 条历史，且是 **unrelated history**——`v0.7.8..HEAD` 里它们全部
可达，不过滤会整个落进「本次发布」（实测桌面 122 条 vs 过滤后 6 条）。两侧都要过滤：

```bash
# 桌面：只认 v* tag，排掉 mobile/
git-cliff --latest --tag-pattern '^v[0-9]' --exclude-path 'mobile/**'
# 移动：只认 mobile-v* tag，只取 mobile/ 与共享 core
git-cliff --config mobile/cliff.toml --latest --tag-pattern '^mobile-v' \
  --include-path 'mobile/**' --include-path 'crates/**' --include-path 'libs/**'
```

> `--include-path 'libs/**'` 曾在 `mobile-release.yml` 两处挂着，`libs/` 于 2026-07 删除后
> 成了 no-op，**已于 2026-07-27 清理**。共享 core 全部由 `crates/**` 覆盖。

`--tag-pattern` 不能省：否则 git-cliff 会把另一条线的 tag 当成上一个版本。
`pnpm changelog` / `changelog:latest` 已内置桌面侧过滤。

**`mobile-v0.7.18` 这个 tag 打在并入点(merge commit)上，不是 RN 的原 HEAD**：后者是
unrelated history 的末端，主仓全部 `crates/` 提交都不在其祖先链上，会被算成「本次发布的
新内容」（实测 81 条 vs 打在并入点的 1 条）。

### SwarmHive 的 app 还记着代码来源，改仓库结构要同步改它

SwarmHive 服务端给每个 app 存了一行 `github_source`（owner / repo / tag template），用于
GitHub 镜像与 liveness 探测。**它是仓库结构的第二份真相，改仓不改它就会发版失败或发错。**

移动端并入单仓时踩到两处，第一处会响、第二处是哑的：

```
# 响的：swarmhive-action 传的 mirror_url 指向主仓，与配置的 source 对不上，exit 2
mirror_url repo swarm-apps/SwarmDrop does not match the app's configured source swarm-apps/SwarmDrop-RN
```

```
# 哑的：tag template 仍是 v{version}。在 RN 独立仓里没问题（那边 tag 就叫 v0.7.18），
# 但在单仓里 v0.7.19 是**桌面**的 tag —— 只改 repo 不改模板，SwarmHive 会去主仓找
# v0.7.19，把「下载 Android」指到桌面的安装包上。
```

**正确做法**（CLI ≥ 0.9.0 才有 `source` 子命令；`apps update` 只能改 display-name / platforms，改不了它）：

```bash
npx @swarm-hive/cli@0.9.0 source get --app swarmdrop-rn   # 读，无需 token
npx @swarm-hive/cli@0.9.0 source set --app swarmdrop-rn \
  --owner swarm-apps --repo SwarmDrop --tag-template 'mobile-v{version}'
```

`--prefer-platform` 省略即保持原值（本项目是 `react-native-android`，别误清）。

**注意 CI 里的版本**：`swarm-apps/swarmhive-action@v2` 内部固定 `@swarm-hive/cli@0.7.0`，那个版本
没有 `source` 命令 —— 改配置要在本地用新版 CLI，别指望 CI 顺手带过去。

服务端校验逻辑见 SwarmHive 的 `crates/swarmhive-server/src/services/mirror.rs`：有 `github_source`
行就必须匹配 owner/repo，没有该行则只校验 URL 是不是合法的 github release-download 链接。

### 补分界 tag 会不会误发版：看 tag 指向的 commit 上有没有 workflow

GitHub Actions 的 tag 触发，判据是 **tag 指向的那个 commit 的树里有没有该 workflow 文件**，
不是默认分支上有没有。

补 `mobile-v0.7.18` 时没有触发发布，靠的是它指向的 merge commit 早于加入
`mobile-release.yml` 的那次提交——**属于巧合，不是设计**。日后再补 `v*` / `mobile-v*` 形式的
分界 tag，若打在已含对应 workflow 的 commit 上，会真的跑一遍构建并发到 SwarmHive。

**要补而又不想发版**，二选一：先确认目标 commit 不含该 workflow；或改用不匹配触发 glob 的
tag 名（如 `mobile-baseline-0.7.18`）。

### Tauri workspace 的 release bundle 在根 `target/`

SwarmDrop 的 `src-tauri` 是 Cargo workspace member，不是独立 Cargo 项目。`tauri-action`
打包后产物位于仓库根目录的 `target/.../release/bundle/`，不是 `src-tauri/target/...`。

**正确做法**：
- SwarmHive `artifact-paths` 同时扫 `target/${{ matrix.swarmhive_target }}/release/bundle/**/*`
  和 `target/release/bundle/**/*`。
- 不要写成 `src-tauri/target/...`，否则 `swarmhive-action@v2` 会选不到 updater bundle，
  CI 在上传步报 `no updater bundles selected`。

**相关文件**：`.github/workflows/release.yml`

### macOS 签名：arm64 的 ad-hoc 是硬性下限，不是可选优化（2026-08-07）

在此之前 CI 完全没有代码签名 —— `TAURI_SIGNING_PRIVATE_KEY` 是 **updater 的 minisign**
（验证更新包完整性），与 Gatekeeper / codesign 是**两件不相干的事**，别把前者当成"已经签过名了"。

后果是 arm64 的 dmg 在 Apple Silicon 上装不了，且报错极具误导性：**「已损坏，无法打开，
你应该将它移到废纸篓」**。文件一点没坏。真实机制分两层：

1. **Apple Silicon 内核硬性要求**：加载可执行页时必须校验页哈希（Code Directory），
   没有就直接 SIGKILL。x86_64 无此限制 —— 这就是为什么同一次构建 Intel 版能装、arm64 版不能。
2. **为什么是"已损坏"而不是"未签名"**：Rust 链接器本来会给 arm64 二进制加 ad-hoc 签名，
   但 bundler 打包时改了 `.app` 内容（Info.plist / 图标 / 资源），哈希对不上，签名**变成损坏态**。
   这比干脆没签名更糟 —— macOS 15 起「已损坏」**没有 UI 绕过路径**（右键打开的快捷方式已被移除）。

三档状态，选型时按这个分：

| 状态 | 表现 | 用户能否自救 | 能否公证 |
|---|---|---|---|
| 签名损坏 / 缺失 | 「已损坏，移到废纸篓」 | ❌ 只能 `xattr -dr com.apple.quarantine` | — |
| **ad-hoc**（当前） | 「无法验证开发者」 | ✅ 系统设置 → 隐私与安全性 → 仍要打开 | ❌ |
| Developer ID + 公证 + staple | 双击直接开 | — 无摩擦 | ✅ |

**正确做法**：`src-tauri/tauri.conf.json` 的 `bundle.macOS.signingIdentity` 设 `"-"`
（`-` 是 codesign 表示 ad-hoc 的特殊标识符）。它让 bundler 在**打包完成后**重新
`codesign -s -`，把哈希表重算一遍。零成本、不需要任何证书、CI 无需额外 secrets。

**字段名易错点**：`MacConfig` 是 `rename_all = "camelCase"`，且外层是
`#[serde(rename = "macOS", alias = "macos")]` —— 写 `macOS`（OS 大写）或 `macos` 都行，
但 `signingIdentity` 的驼峰不能错。好在该结构体带 `deny_unknown_fields`，
拼错会在 `cargo check -p swarmdrop`（tauri-build 解析配置）当场报错，不会静默忽略。

**`hardenedRuntime` 默认就是 `true`**（`tauri-utils` 的 `MacConfig::default`），
所以 ad-hoc 会以 `--options runtime` 签。刻意**没有**改成 false：hardened runtime 的唯一
实际用途是公证的前置条件（ad-hoc 根本不能公证，理论上没收益），但 Tauri 官方默认如此、
大量应用这么跑，且将来切 Developer ID 时正好需要它为 true。若实测出现 library validation
类的加载失败，再考虑显式关掉。

**未验证、需要实测的一条**：keychain 的访问控制绑定签名身份，ad-hoc 的 designated
requirement 是内容哈希（cdhash）、**每次构建都变**。`keyring` 里存的 Ed25519 设备私钥
可能在每次更新后都要用户重新授权一次。Developer ID 签名不会有这问题（身份绑 Team ID，
跨版本稳定）。**首次发 ad-hoc 版本前，务必拿旧版 → 新版实跑一遍升级路径确认。**

**将来切 Developer ID**：把 `"-"` 换成 `"Developer ID Application: NAME (TEAMID)"`，
CI 补 `APPLE_CERTIFICATE` / `APPLE_CERTIFICATE_PASSWORD` / `APPLE_SIGNING_IDENTITY`，
公证再加 `APPLE_API_ISSUER` / `APPLE_API_KEY` / `APPLE_API_KEY_PATH`（比 `APPLE_ID` +
app 专用密码稳，不随密码轮换失效）。需 Apple Developer Program，$99/年 —— **没有免费替代**，
免费 Apple ID 签出来的东西 7 天过期且不能分发，ad-hoc 就是免费档的上限。

Windows 侧另说：Authenticode 私钥自 2023-06 起强制硬件存储，`.pfx` 不能再塞进 CI secrets。
开源项目可申请 **SignPath Foundation** 免费签名（要求 OSI 许可证 + 公开可追溯构建 +
每次发布手动批准 + 只支持 GitHub Actions / GitLab CI）。

**相关文件**：`src-tauri/tauri.conf.json`、`.github/workflows/release.yml`

### mobile-release.yml 缺两条 iroh-ffi 已验证的 CI 实践

2026-07 读 iroh-ffi 的 CI 时发现两条我们缺、且**与迁不迁 iroh 无关**的实践，可直接抄：

**① 可复现构建 —— 我们现在 .a 里嵌着绝对路径，泄露且不可复现**

iroh-ffi 在 RUSTFLAGS 里加 4 条 `--remap-path-prefix`（cargo registry / cargo git / 源码 checkout /
rustup sysroot），**并且**在 CFLAGS 里加 3 条 `-ffile-prefix-map`。

⚠️ 第二半不能省：`--remap-path-prefix` 是 **Rust-only** 的，`ring` 等依赖走 build.rs + `cc`
编译 bundled C 源码，只有 `-ffile-prefix-map` 管得到它们。

**② 发布前验证产物形状 —— 我们现在只验「构建成功」**

iroh-ffi 有 `cargo make verify-swift-xcframework` / `verify-kotlin-android-consumer` /
`verify-kotlin-consumer`，Makefile.toml 注释里写明动机，抓的正是
*"succeeds, artifact is broken, runtime crash on consumer device"* 这一类。

具体到 Android：把刚构建的 .so 塞进一个**真的 consumer app**，在 emulator 上跑 instrumented test
（纯离线的一行调用即可）。抓三类构建期看不见的问题：

- AGP 没把 .so 从 JAR merge 进 APK
- .so 加载了但 JNI 符号缺失
- NDK API level 对 emulator 太高

**相关文件**：`.github/workflows/mobile-release.yml`

### pnpm/action-setup 不能与 packageManager 双指定

`pnpm/action-setup` 的 `with: version:` 和 `package.json` 的 `packageManager` 字段**不能同时存在**，否则报 `Multiple versions of pnpm specified` / `ERR_PNPM_BAD_PM_VERSION`，CI 在 Setup pnpm 步骤直接失败。

**正确做法**：SwarmDrop（有 `packageManager: "pnpm@11.10.0"`）的 workflow 里 `pnpm/action-setup@v4` **不要带 `with: version`**，让它读 packageManager。RN（无 packageManager 字段）靠 action 的 `version` 指定。别混用。

### windows updater bundle 选取：用清单内匹配，别用 `[ -f ]`

`release.yml` 的 "Pick SwarmHive updater bundle" 从 tauri-action 的 `artifactPaths` 里挑「有同名 `.sig`」的 updater bundle。两个坑：(1) **windows 的 `D:/...` 盘符路径在 Git bash 下 `[ -f "${f}.sig" ]` 不可靠**（漏判 → updater 选空 → exit 1）；(2) tauri v2 windows updater 产物是 `-setup.exe` + `.exe.sig`（**没有 `.nsis.zip`**）。

**正确做法**：把清单 `jq -r '.[]' | sed 's#\\#/#g'` 转正斜杠存进 `paths`，用 `grep -qxF "${f}.sig" <<< "$paths"` 在清单内判断有无同名 sig；windows 优先选 `*-setup.exe`。

### 移动端已并入单仓（mobile/），core 是普通 path 依赖

> 历史：移动端曾是独立的 `swarm-apps/SwarmDrop-RN` 仓，靠 git rev pin 引用主仓的
> `swarmdrop-core`/`entity`/`migration`/`swarm-p2p-core`，本地联调要手工把四行 git 改成
> path、改完再改回去。该模式已废弃——它让 core 的改动漂到移动端要走一次「同步仪式」，
> 实测漂了 6 天、漏掉一个已修的配对校验。现在 `mobile/` 是主仓的一个目录。

- `mobile/packages/swarmdrop-core/rust/mobile-core` 是**根 Cargo workspace 的 member**，
  四个依赖都写 `{ workspace = true }`。改 `crates/core` 立刻对移动端生效，
  `cargo check --workspace` 一并覆盖，一个 PR 能同时改两端并原子回滚。
- `mobile/` 是**独立的 pnpm workspace**（同 `video/`、`e2e/desktop`、`docs/`），
  有自己的 `package.json` / `pnpm-lock.yaml` / `pnpm-workspace.yaml`，在 `mobile/` 下跑
  `pnpm install`。根 workspace 的 `packages: [.]` 不含它。

**移动端 release profile 必须写在 workspace root**：

```toml
# 根 Cargo.toml —— 不能写成 [profile.release]，那是桌面壳的
[profile.mobile-release]
inherits = "release"
lto = "thin"
codegen-units = 1
opt-level = 3
strip = "symbols"
```

**Why 必须在 root**：Cargo 只认 workspace root 的 profile，**member 自己的 profile 会被
静默忽略**（只有一行 warning）。mobile-core 并入前是隐式 workspace root、自带这套配置；
并入后若不搬到根，移动端的 profile 就无声消失。消费方是
`ubrn build <platform> --profile mobile-release`（ubrn 的 `-p` 覆盖 `-r`），产物落在
`target/mobile-release/` 而非 `target/release/`。ubrn 用 `cargo metadata` 的
`target_directory` 定位产物，会自动跟到仓库根，无需额外配置。

### `opt-level` 从 `"z"` 改回 3，并放弃「逐包例外」这条路（2026-08-10）

这个 profile 直到 2026-08-10 都是 `opt-level = "z"`（包体优先）。**两次独立实测证明它在
传输热路径上的代价是数量级的**，而第二次同时证明了「开单包例外」是打地鼠：

1. **blake3**——`"z"` 在这个包上**穿透了 Rust 边界**：它的 `build.rs` 里
   `!is_no_neon() && !is_pure() && is_aarch64() && is_little_endian()` 会
   `build_neon_c_intrinsics()`，即 **iOS/Android 的 arm64 一律走 C 实现**
   （`c/blake3_neon.c`），而 cc crate 把 profile 的 `opt-level` 原样翻译成给 clang 的
   `-Oz`（实测确认）。被按住的不是几 KB Rust 代码，是那份 intrinsics 的内联与展开。
2. **WebRTC 数据面**——DTLS 记录层走 RustCrypto 的**纯 Rust** 实现
   （`aes` / `aes-gcm` / `ghash` / `polyval`…），不是 ring / aws-lc 那样的 asm；SCTP 每个
   包还要算 `crc`。纯 Rust 的 AES-GCM 与 GHASH 高度依赖内联与循环展开，而 `"z"` 恰好把
   这两样都关掉。

第 2 条解释了三端实测里一条否则说不通的分裂：**同一台 Android 手机**，走 QUIC/Noise 是
12–23 MB/s，走 WebRTC 掉到 0.36–0.96 MB/s；而同一条 WebRTC 链路换成 `opt-level = 3` 的
桌面端做对端就有 6–10+ MB/s。**差别正是「加密走 asm（quinn/rustls → ring）还是走被 `-Oz`
阉割的纯 Rust（webrtc-rs → RustCrypto）」**——依赖树里混着两种加密实现，而 profile 只
影响得了后者。

**为什么不继续开单包例外**：判据本身（「按字节计费的热点」）是对的，问题是这类热点遍布
整棵依赖树——哈希、AEAD、GHASH、CRC、分片、编解码……逐个列既列不全（每次只在真机实测
里冒头一个），也会让「为什么偏偏是这几个包」变成一笔谁也不敢动的糊涂账。传输吞吐是本
App 的核心功能，不是可以拿来换几 MB 的东西。

体积改由 `lto = "thin"` + `codegen-units = 1` + `strip = "symbols"` 承担——这三项都
**不牺牲**运行速度（前两项还会提升）。

> 与 `[profile.dev.package."*"]` 的 `opt-level = 3` 仍是**两件事**：那条管 dev 构建，
> 这条管移动端 release 产物。两者现在恰好都是 3，理由不同，不要合并。

⚠️ **`opt-level = 3` 目前仍是待验证的假设。** 慢的那条链路同时还差着「打洞 vs
webrtc-direct」这个未分离的变量，只有重测同一条打洞链路才能定案。若实测证明与吞吐无关，
回退前先把结论写回 `dev-notes/research/2026-08-10-v0.15.2-field-test.md`。

⚠️ **profile 与单包覆写都只有 workspace root 的算数**——写在 `mobile-core/Cargo.toml` 里
不会报错，只是不生效，而「不生效」在这里没有任何可观测信号（包能编、能跑，只是慢）。

**相关文件**：`Cargo.toml`、`mobile/packages/swarmdrop-core/rust/mobile-core/Cargo.toml`、
`mobile/packages/swarmdrop-core/package.json`

### expo-file-system 56.0.8 的 SAF FileHandle 必须保活 PFD

Android 接收位置选系统目录后走 SAF `content://`。expo-file-system 56.0.8 的
`FileSystemFileHandle.forContentURI` 只从 `ParcelFileDescriptor.fileDescriptor` 创建
`FileChannel`，却不在 handle 中持有 `ParcelFileDescriptor`；PFD 被 GC 回收后，仍在使用的
channel 会随机变成 `Bad file descriptor`。传输层随后只看到 data stream 被关闭，发送端常显示
`connection is closed`，发生字节数取决于 GC 时机，**不是 100 MB 限制或缺少背压**。

项目通过 `mobile/pnpm-workspace.yaml` 的 `patchedDependencies` 修复：

- 持有 PFD 到 `FileHandle.close()`，并在 finally 中同时关闭；该部分来自 expo/expo#47176。
- `read` / `write` 失败带上**异常类名**（`describe(e)`）。上游只取 `e.message`，而写失败最
  要紧的那几种恰好 message 为 null（fd 失效的 `IOException`、`ClosedChannelException`），
  丢了类名就只剩一句 `unknown error`。这条同时是「补丁到底有没有进构建」的**运行时探针**：
  日志里的 reason 若是**裸** `'Bad file descriptor'`（无类名），说明吃的是未打补丁的版本。
- `offset` setter 补 `ensureIsOpen()`，与 read/write 对齐。
- **SAF 的 `"rw"` 保持上游的拒绝**——`FileOutputStream.getChannel()` 是 readable=false 的
  channel，拿它冒充 "rw" 会得到一个「签名说能读、实际不能读」的句柄。接收侧也不再需要它：
  staging 恒在应用私有目录（走 `forJavaFile` 的 `RandomAccessFile`），SAF 只在 publish 时被
  **顺序**写一次。
  > 本节此前写作「续传用 `"rw"` 保留已有内容并做可定位写」——**那与 patch 的实际内容相反**，
  > 2026-08-10 更正。

补丁配置必须留在 `pnpm-workspace.yaml`（pnpm 11 不读 package.json 的 `pnpm` 字段），更新后用
`pnpm install --config.allowUnusedPatches=true` 并确认 `mobile/pnpm-lock.yaml` 出现对应
`patch_hash`。升级 expo-file-system 时先核对上游是否覆盖 PFD 生命周期，不能只看到 #47176 合入
就直接删除整个补丁。

⚠️ **但这份补丁在 2026-08-10 之前从未真正进过 Android 构建**，原因见下一节——
`pnpm install` 全绿、`node_modules` 里的 Kotlin 确实是打过补丁的，构建吃的却是预编译 AAR。

**相关文件**：`mobile/patches/expo-file-system@56.0.8.patch`、
`mobile/src/core/foreign-file-access.ts`、`mobile/pnpm-workspace.yaml`

### pnpm patch 打在有预编译产物的原生依赖上会**静默失效**（2026-08-10 实证）

上一节那份补丁写于 2026-08-07，改了三次、三次都以为修好了，**三次都是空的**：Android 构建
拿的是 expo 发布的预编译 AAR，`node_modules` 里被 patch 过的 Kotlin 源码从来没参与编译。
代价不只是 bug 没修——它还伪造出一条「架构事实」：因为「补丁都打了还是崩」，团队把它归因成
「SAF 的 fd 天生不能用」，并把这条误诊写进了 `CLAUDE.md` 与三份知识库（已于同日更正，见
[rust-backend.md](rust-backend.md) 的「接收是『暂存 → 发布』两段」）。

**机制**：expo SDK 53+ 的 autolinking 对**发布了 maven / CocoaPods 产物**的模块默认走
publication 而不是源码工程：

```
$ expo-modules-autolinking resolve -p android --json
expo-file-system publication={groupId: host.exp.exponent, …, repository: local-maven-repo}
                 shouldUsePublicationScriptPath: None
```

`shouldUsePublication` → `true` → `linkProject()` 被跳过 → gradle 只 `implementation` 那个
AAR。同款坑在 Apple 侧一模一样（`platforms/apple/apple.js` 也吐 `buildFromSource`，
预编译 XCFramework 会吃掉 `ios/` 下的 patch）。

**修法**（`mobile/package.json` 顶层 `expo` 键，本仓已加）：

```jsonc
{ "expo": { "autolinking": { "android": { "buildFromSource": ["expo-file-system"] } } } }
```

**判据必须是编译产物里的符号，不是源码**。这是本条最值钱的一句：

```bash
# 源码打没打补丁（会骗人——它永远是打过的）
grep parcelFileDescriptor node_modules/expo-file-system/android/src/main/java/**/FileSystemFileHandle.kt

# 构建到底吃了什么（唯一可信）
javap -p node_modules/expo-file-system/android/build/**/FileSystemFileHandle.class | grep -E 'parcelFileDescriptor|describe'
```

**通用判据：一个补丁的验收，必须落在「构建产物」这一侧。** 凡是依赖同时发布了预编译产物
（AAR / XCFramework / prebuilt `.node` / wasm blob），「patch 应用成功」与「patch 参与构建」
就是两件事，而包管理器只报前者。同族的坑：本文下面那节「本地 expo module 的 Kotlin 不在任何
门禁里」、[net-kernel.md](net-kernel.md) 记的「自研替换掉一个曾打过补丁的上游实现时，补丁
不会自己跟着走」——三条都是「改动看起来在、实际没进产物」。

机器护栏：`mobile/scripts/check-expo-patches.mjs`（`javap` 符号断言必选、覆盖 Apple 侧的
`buildFromSource`、只检查改动触及 `android/` 或 `ios/` 的 patch、`patchedDependencies` 从
`mobile/pnpm-workspace.yaml` 读且 resolve 必须在 `mobile/` 下跑）。
**这类事故只有机器守得住**——三个 commit、三次自评「已修复」，人工 review 一次都没拦下。

### `biome.json` 里写注释 = 整份配置**静默失效**（2026-08-13 实证）

同族的第三条：上一节是「补丁没进构建」，这条是「配置没进生效路径」，判据同样不能看
「我写了吗」，要看「它读到了吗」。

`biome.json` 是**严格 JSON**，`//` 注释是语法错误。而 biome 撞上这种配置**不报错，
默认降级到内置配置继续跑** —— 于是：

| | 真配置（`indentStyle: space`、`files.includes` 只列 ts/tsx） | 降级后（biome 默认） |
|---|---|---|
| 缩进 | 2 空格 | **tab** —— 全仓每个文件都「需要格式化」 |
| 扫描范围 | `src/**` + `packages/**` + `modules/**` 的 ts/tsx | **`**`** —— 连 `modules/*/android/build/intermediates/**.json` 这种 Gradle 产物和 `global.css` 都进去了（后者还报 parse error） |
| 错误数 | 2 | **195** |

那 195 差点被当成「移动端 lint 早就烂了」而按既有债处理掉。真相是它**完全由那几行注释造成**
——`git stash` 掉配置改动重跑一次基线就露馅了。**错误数突然一个数量级的跳变，先怀疑配置没读到，
别急着修代码。**

两条判据：

```bash
pnpm exec biome rage | grep -A1 "Biome Configuration"   # Status: Not set = 没读到
pnpm exec biome ci src/ --config-path=.                 # 显式指定时解析失败会**硬报错**（带行号）
```

修法：**改用 `biome.jsonc`**（biome 自动识别，脚本不用改）。本仓需要在配置里写「为什么豁免」，
所以是 jsonc 而不是删注释 —— 一条没有理由的 lint 豁免，下一个人只会把它删掉。

### 一条没人跑的检查会退化成噪音（2026-08-13）

`mobile/` 的 `typecheck` / `lint:ci` / `check:zustand-access` 三条同时红着，而
`mobile-checks.yml` 只跑 `check:expo-patches` —— workflow 自己的注释里写着「将来也归这儿」，
挂了很久。三条补进 CI 时的实际状态：biome 5 处、zustand 10 处。

**zustand 那 10 处里有一多半根本不是新违规**：allowlist 是按 `{file, pattern}` 匹配的，其中
一条写的是 `usePairingCodeStore`，而 6 位分享码换成 PairInvite 时 store 改名成了
`usePairingInviteStore`。规则本身没错，只是它指向的东西不存在了 —— 于是同一段 orchestration
代码一夜之间变成两条「违规」，而没人在跑它。同理 `src/core/paths.ts` 的豁免没跟到继任者
`receive-location.ts`。

**推论：正则里嵌了符号名的护栏，改名时必须一起改**，而保证这件事发生的唯一办法是让它在
CI 里红。护栏脚本本身也要留下这条线索（本仓已在
`mobile/scripts/check-zustand-store-access.mjs` 那条 allowlist 上写明）。

顺带一提，那 10 处里真正该改的只有 3 处，而且改法不是加豁免：`previewInvite` 返回
`boolean`、失败原因另放在 store 的 `previewReject` 字段里，于是三个调用点各自
`getState().previewReject` 再抄一遍同样的三元链。**把结果并回返回值**（`Promise<"ok" | 判别码>`）
一次消掉三处违规 + 三份重复 + 一个只写不读的 state 字段。**违规扎堆出现时先看它们像不像同一个
设计问题的三个症状**，那比加三条豁免值钱。

### apt 的 binaryen 会把 wasm 产物**编坏**——Web 端一加载就 `Table.grow` 失败（2026-08-13 实证）

线上 Web 端一进页面就报，节点起不来：

```
RangeError: WebAssembly.Table.grow(): failed to grow table by 4
    at __wbindgen_init_externref_table
```

**本地怎么试都复现不了**，因为坏的产物只在 CI 里生成。

**根因是 wasm-opt 的版本。** `docs.yml` 此前为了摆脱「wasm-pack 裸下载 binaryen」那条不稳定的
网络依赖（无重试无缓存，2026-08-12 报 `failed to download binaryen-version_117` 挂过一次），
改成了 `apt-get install binaryen`——而 **Ubuntu noble 的 binaryen 停在 `108-1`**（2022 年）。
那个版本优化时重排了 table，却**没有重映射导出索引**：

| | `__wbindgen_externrefs` 指向 | 结果 |
|---|---|---|
| 本地（wasm-pack 自己拉的新版） | table#1 · externref · `max` 为空 | `grow(4)` 正常 |
| CI（apt binaryen 108） | table#**0** · **funcref** · `min == max == 3598` | `grow(4)` 必抛 |

JS glue 里那句 `wasm.__wbindgen_externrefs.grow(4)` 拿到的是一张**满的 funcref 表**，
物理上不可能增长。错误信息只说「grow 失败」，完全指不到「导出索引错了」。

**判据（不用装任何工具，解析 wasm 二进制即可）**：读 table section（id=4）与 export
section（id=7），确认 `__wbindgen_externrefs` 指向的那张表是 `externref` 且**没有 `max`**。
funcref 表恒为 `min == max`，一眼可辨。

**现在的做法**：`docs.yml` **不再在 CI 里重新生成 wasm**，改吃入库的
`packages/swarmdrop-web/`（docs 以 `link:` 引着）。binaryen 的**版本**与**下载稳定性**
两条依赖一起消失，顺带省掉几分钟 wasm 编译。

**不要做**：
- 别再往 CI 里加 `apt-get install binaryen`。发行版的 binaryen 落后 wasm-bindgen 太多，
  失败形态是**产物看起来正常、运行时才炸**，比编译失败难查一个量级。
- 若将来要恢复 CI 重建，binaryen 必须**锁定一个新版本号**从 GitHub release 取（带
  `curl --retry`），不能用发行版包，也不能让 wasm-pack 裸下载。

**代价与兜底**：入库产物可能过期（改了 wire 却忘了重建 → 线上 Web 端静默停在旧协议）。
`scripts/check-wasm-artifact.sh` 在 rust.yml 的 wasm job 里拦这件事，输入面与
`check-wasm.sh` 的 `CRATES` 对齐（另加 `crates/entity` 与 `Cargo.lock`）。源码动了但产物
字节确实不变时，用 commit message 里的 `[wasm-artifact-unchanged]` 放行——刻意要求留痕。

⚠️ **那个逃生舱比它读起来窄得多，默认按「老实重建」办。** 判据是「字节不变」，而
wasm 里嵌着 panic location 元数据（`file:line`），**任何往文件中间插代码的改动都会挪动
行号、从而改掉产物字节**。实测一次：`crates/transfer/src/inbox.rs` 加了一个连 wasm 侧都
没人调用的 `pub fn`，重建后产物**大小一字节没变**、内容差 **8 个字节**，全是行号
（`inbox.rs:503` → `inbox.rs:537` 及邻近三处）。所以逃生舱实际上只对「在文件末尾追加」
或「只动 native-only 文件」成立；判不准就重建一次，用下面这段直接看差在哪：

```bash
git show HEAD:packages/swarmdrop-web/swarmdrop_web_bg.wasm > /tmp/old.wasm
cd docs && pnpm build:wasm && cd ..
python3 -c "
old=open('/tmp/old.wasm','rb').read(); new=open('packages/swarmdrop-web/swarmdrop_web_bg.wasm','rb').read()
d=[i for i in range(min(len(old),len(new))) if old[i]!=new[i]]
print(f'长度 {len(old)} → {len(new)}，不同字节 {len(d)}')
print(old[d[0]-24:d[0]+40], '\n', new[d[0]-24:d[0]+40]) if d else None"
```

差异只有几个字节且落在路径字符串附近 = 纯行号漂移，功能没变；但**产物仍然要提交**，
否则下一次 push 还会红。顺带：这也说明 wasm 构建是**字节可复现**的——同一份源码重建
两次结果相同，差异都能归因到源码。

**`crates/*/tests|examples|benches/` 与 `*.md` 已自动排除**（2026-08-20 起）：改一条
e2e 测试曾经也要求重建 4.8 MB 二进制。排除规则写成黑名单而不是白名单是刻意的——白名单
对**新出现的输入种类**默认放行，而本脚本存在的全部理由就是防漏报。反例就在手边：
`crates/web/bindings/bindings.ts` 被 `src/node.rs` 用 `include_str!` 吃进二进制，
它既不在 `src/` 下也不是 `.rs`，白名单会直接漏掉。目录部分只认 crate 顶层，
不写 `(^|/)tests/`——后者会连 `src/**/tests/` 这种**编进 lib 的模块目录**一起吃掉。

**改了 wasm 侧 crate 的工作流**：`cd docs && pnpm build:wasm`，把产物一起提交
（历史上那些 `chore(web): 重建 wasm 产物` 就是它）。

**相关文件**：`.github/workflows/docs.yml`、`.github/workflows/rust.yml`、
`scripts/check-wasm-artifact.sh`、`packages/swarmdrop-web/`

## 跨三个 workspace 共享 TS 包：`packages/shared-view`

三端共享的**纯视图逻辑**（设备显示投影 + 格式化）住在仓库根的 `packages/shared-view`。
「什么该进这个包、什么不该」的判据写在**该包自己的 README** 里，别在这里找；这一节只记
跨 workspace 接线的坑。

### 发布 TS 源，不预构建 —— 但那不是「零配置」

包的 `exports` 直接指向 `src/index.ts`，三端各付一行构建配置：

| 端 | 声明 | 构建配置 |
|---|---|---|
| 桌面（Vite） | 根 workspace member，`workspace:*` | 无 |
| Web（Next / turbopack） | `link:../packages/shared-view` | `transpilePackages` + **`turbopack.root` 放到仓库根** |
| 移动（Metro） | `link:../packages/shared-view` | `watchFolders` 加 `../packages` |

### turbopack 的 `root` 是文件系统边界，不只是 lockfile 探测起点

`docs/next.config.mjs` 的 `turbopack.root` 原先锁在 `docs/`（为消除多 lockfile 警告）。
共享包在仓库根，落在那个边界之外，于是：

- `pnpm typecheck` **全绿**（tsc 沿 symlink 解析得到）
- `next build` 报 `Module not found: Can't resolve '@swarmdrop/shared-view'`

**只有构建那一步会红**，类型检查给不了任何提示。修法是把 root 指向仓库根——显式指定同样没有
多 lockfile 警告（歧义来自推断，不是位置）。

**预构建成 `.js` 救不了这条**（实测验证过）：失败的是 *resolution*，与产物是 `.ts` 还是 `.js`
无关。所以 tsdown / tsup 之类只能省掉 `transpilePackages` 一行，省不掉 root——权衡时别把它
算成收益。

### 那条放宽的代价：Next < 16.3 的 dev 会吃光内存把机器搞重启（2026-08-05 实测）

root 放到仓库根意味着 turbopack 把**整个仓库**纳入文件系统边界——包括 173G 的 `target/`、
15G 的 `mobile/` 和散落的 686 个 `node_modules`。Next **16.2.6** 下这笔账在**首次编译任意
路由**时结清（不是启动时——`Ready in 136ms` 之后内存才起飞，容易误判成「启动没问题」）：

| 配置（同一路由 `/docs/`，16G 机器） | 峰值 | 结果 |
|---|---|---|
| 16.2.6 turbopack，root=仓库根 | **11 G 仍在陡升** | 熔断前未编译完；不干预即吃光内存**系统重启** |
| 16.2.6 turbopack，root=`docs/` | 4.9 G | 完成 |
| 16.2.6 **webpack**（`next dev --webpack`） | 2.5 G | 完成 |
| **16.3.0 turbopack，root=仓库根** | **2.4 G** | 完成 |

**修法就是升到 Next ≥ 16.3.0**，配置一行不用动——16.3 的 Turbopack 加了内存驱逐（非活跃
路由换出到磁盘），官方口径「大型应用 dev 内存降约 90%」，本仓实测降幅同量级。

两条别走弯路的结论：

- **元凶不是 `target/`**。把 173G 的 `target/` 整个移出仓库再跑，照样爆到 11G——代价来自
  root 变宽这件事本身（解析面 + 686 个 `node_modules`），不是某个大目录。所以
  「清 target 就好了」是错的。
- **`--webpack` 是留给旧版本的应急阀**，不是长期方案：它同样能编完且只吃 2.5G，但拿不到
  turbopack 的编译速度。升上 16.3 之后不需要它。

**相关文件**：`docs/next.config.mjs`、`docs/package.json`

### 「零平台依赖」要两道门，`lib` 一道不够

包的 `tsconfig` 用 `lib: ["ES2022"]`（无 DOM、`types: []`）挡住 `document.` / `window.`，
但**挡不住 `import { useState } from "react"`**：包嵌在仓库根之下，tsc 的模块解析会一路向上
走到**仓库根的 `node_modules`** 并解析成功。pnpm 的 isolated 链接兜不住这件事。

第二道是 `scripts/check-shared-view-imports.mjs`：非测试源文件只允许相对路径 import。
两道合并在 `pnpm check:shared-view`，**要留在提交前清单里**——第一道只在对该包自身跑 tsc 时
成立，三端各自 typecheck 用的是各自的 lib。

### 定时器一类的平台能力：**参数化，且注入时要包一层箭头函数**（2026-08-10 实证）

`packages/shared-view/src/transfer/session-timers.ts` 是本包第一个需要**平台能力**（而不只是
纯计算）的原语——会话级定时器台账。上面那条 `lib: ["ES2022"]` 的门禁意味着 `setTimeout` 在
这个包里**根本不存在**，所以调度器只能由调用点注入：
`createSessionTimers<H>(setTimer, clearTimer)`。

**注入时不能把 `setTimeout` 当值传。** 台账通常是模块级常量，
`createSessionTimers(setTimeout, clearTimeout)` 等于在**模块求值那一刻**把全局函数快照下来：

- `vi.useFakeTimers()` 替换的是 `globalThis.setTimeout`，而快照里握着的是真时钟 ⇒ 所有靠
  `advanceTimersByTime` 推进的用例一起红，症状是「到点该发生的事成片不发生」，**看起来像
  业务逻辑坏了**。桌面与 Web 在同一天各自独立踩了一次，两边都先怀疑共享原语有问题。
- 浏览器里 `setTimeout` 是 WebIDL 方法，脱离 `window` 单独调用会 `Illegal invocation` ——
  这条纯运行时，测试根本照不到。

正确写法是让全局在**调用时**解析：

```ts
const timers = createSessionTimers(
  (fire, delayMs) => setTimeout(fire, delayMs),
  (handle) => clearTimeout(handle),
);
```

推论：以后往这个包放任何「注入平台能力」的原语，签名文档里都要把这句写上——它不是调用风格
问题，是两个不同的真实故障。

### `packages/` 统一在仓库根

`swarmdrop-web`（wasm 产物）此前住在 `docs/packages/`，现已移到 `packages/swarmdrop-web`，
与 `shared-view` 同级。`docs/pnpm-workspace.yaml` 因此退化成只有 `.` 一个成员，两个共享包
都用 `link:` 引用。

这个移动**依赖上面那条 turbopack root 的放宽**——包移出 `docs/` 后同样落在原 root 之外。

### 有运行时 import 的共享包，独立 workspace 必须用 `file:` 而不是 `link:`（2026-08-06 实证）

上面「零平台依赖要两道门」那条说的是 **tsc** 会一路向上解析到仓库根的 `node_modules`。
**同一个机制在运行时同样成立，而且后果严重得多。**

`shared-view` 一直用 `link:` 没出过事，是因为它**零运行时 import**。第一个有运行时依赖的
共享包（`packages/file-browser`，React DOM 组件）照抄 `link:` 后，`docs` 的 `next build`
在**预渲染阶段**炸：

```
✓ Compiled successfully in 3.7s        ← 宏展开、转译全都正常
...
TypeError: Cannot destructure property 'i18n' of 'j(...)' as it is null.
  at ../packages/file-browser/src/xxx.tsx
```

不是没编译，是**运行时实例分裂**。`link:` 只是软链，解析真实路径后从
`packages/file-browser/src/` 向上找 `node_modules`：

| 解析起点 | 落到 |
|---|---|
| `packages/file-browser/src` | 仓库根 `@lingui/react@5.9`、`react@19.2.4` |
| `docs/app/app` | `docs/node_modules` 的 `@lingui/react@6.6`、`react@19.2.7` |

两个物理副本 = 两个 `React.createContext` = 组件读到的 context 恒为 `null`。

**别以为对齐版本就能修**：根 workspace 与 `docs/` 各有自己的 `.pnpm` 目录，同版本也是两份
物理副本，React 按文件路径判定模块身份。

**也别以为这只是 Lingui 的事**：`react` 本身就在分裂名单里，所以**任何带 hooks 的共享组件**
都会撞上（`useState` 从错误的 dispatcher 读 → "Invalid hook call"）。上面这个 case 先炸在
Lingui 上纯属巧合——探针组件只用了 `useLingui`。

**修法**：独立 workspace 侧改用 `file:`。

```jsonc
// docs/package.json
"@swarmdrop/shared-view": "link:../packages/shared-view",   // 零 import，link: 够用
"@swarmdrop/file-browser": "file:../packages/file-browser", // 有运行时 import，必须 file:
```

`file:` 让 pnpm 把包装进 **docs 自己的虚拟 store**
（`docs/node_modules/.pnpm/@swarmdrop+file-browser@file+..+packages+file-browser/`），
解析上下文随之变成 docs 的依赖树，两边落回同一份副本。

**代价（会咬人的那条）**：pnpm 对 `file:` 目录依赖用**硬链接**，而硬链接在实践中根本不同步
——**改了共享包就必须重跑 `cd docs && pnpm install`，无一例外**。

硬链接共享 inode，所以「原地改内容」两边确实都能看到。问题是几乎没有工具原地改：编辑器、
`Edit` 工具、格式化器都是写临时文件再 `rename()` 覆盖，新文件是新 inode，链接当场断，
`docs/node_modules/` 里留着的还是旧副本。2026-08-06 实测：给 `FileBrowserActions` 加了个
`onDownload`，docs 侧 `tsc` 报「`onDownload` does not exist in type」，源文件里明明有；
`ls -li` 两边 inode 已经不同。

**症状会伪装成别的问题**，别顺着表象查下去：

| 症状 | 真因 |
|---|---|
| Next `Module not found` | 新增了文件 |
| `tsc` 说某属性不存在，可你刚加了它 | 改了已有文件 |

两种都是先 `pnpm install`（几秒），再怀疑代码——别去查 `transpilePackages` 或 turbopack root。

**判据**：`packages/*` 下的包，**只要有任何非类型的 `import`**，被 `docs/` 或 `mobile/` 消费时
就得用 `file:`。反过来，能守住「零运行时 import」的包（如 `shared-view`）继续用 `link:`——
它少一层硬链接同步的麻烦。

**相关文件**：`packages/shared-view/README.md`、`packages/file-browser/README.md`、
`docs/next.config.mjs`、`docs/package.json`、`mobile/metro.config.js`、
`scripts/check-shared-view-imports.mjs`

## 本地 expo module 的 Kotlin 不在任何门禁里（2026-08-03 实证）

`mobile/modules/*` 下的原生模块（`content-share`、`lan-multicast`）**没有任何一条常规检查会
碰它们的 Kotlin**：`pnpm typecheck` 只看 TS，`pnpm lint`（biome）只扫 `src/`，
`cargo check --workspace` 与 Rust 无关，CI 的 `mobile-build-android.yml` 只在 `mobile-v*` tag
或手动触发时跑。**改了 Kotlin 而不手动编一次，第一次发现问题就是在打 tag 之后。**

改 `mobile/modules/*/android/**` 后至少跑一次单模块编译（不碰 Rust，约 20 秒）：

```bash
cd mobile/android && ./gradlew :<模块目录名>:compileDebugKotlin --console=plain
```

模块名就是 `mobile/modules/` 下的目录名（autolinking 用它当 gradle project 名）。
新建模块后先确认它被发现：

```bash
cd mobile && npx expo-modules-autolinking search --platform android
```

### 坑：`Function` 的 body 里不能用裸 `return@Function`

expo module 的 `Function(name) { … }` body 返回类型是 `Any?`（返回值要过 JSI 桥），而 Kotlin
里**不带值的 `return` 只在返回类型为 Unit 时合法**：

```kotlin
Function("acquire") {
  if (alreadyDone) return@Function                    // ❌ expected 'Any?', actual 'Unit'
  val x = something() ?: return@Function              // ❌ 同上
}
```

用嵌套判断表达同样的意思，或显式 `return@Function null`。前者更干净。

**相关文件**：`mobile/modules/lan-multicast/`、`mobile/modules/content-share/`
