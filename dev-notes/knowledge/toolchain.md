# Toolchain

## 概览

构建 / 包管理 / lint / CI 的项目特有约束。常规命令参考 CLAUDE.md "Build and Development Commands"；本主题只记非显见的坑。

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

### `target/` 是 10G 量级，跑全量测试要留够盘

`opt-level = 3` + libp2p / webrtc-rs / tauri 三棵大依赖树的合并后果：`cargo clean` 之后
跑一轮「check + 相关测试 + wasm 双 target + 桌面 bindings 导出 + mobile cdylib」就能重新
长到 **10–11G**。`cargo test --workspace` 更吃——它要为每个 test target 各编一个二进制。

盘紧时的取舍，按性价比排：

| 手段 | 腾出 | 代价 |
|---|---|---|
| `rm -rf target/debug/incremental` | ~3.7G | 只影响下次增量编译速度，**最安全** |
| `CARGO_INCREMENTAL=0` 跑 | 不再增长 | 单次编译略慢，适合一次性的门禁跑 |
| `cargo clean` | ~10G | 全量重编（libp2p 那棵树十几分钟） |

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
| `crates/web/bindings/bindings.ts` | specta | `cargo test -p swarmdrop-web --features specta --test specta_export` |
| `mobile/packages/swarmdrop-core/src/generated/` | uniffi (ubrn) | `ubrn build ios/android --and-generate`（要 Xcode / NDK） |

2026-08-01 那次简化里，三份**同时**被发现落后于已提交的 Rust：桌面那份还带着已删的
`pauseTransfer`、`start` 的参数个数也不对；uniffi 那份根本没有 `renameDevice` /
`PairedDeviceRemoved` / `DeviceRenamed`，`startNode` 还带着已删的 `deviceName` 参数。
前端能跑只是因为它调用的恰好是交集。**按移动端的失败模式，落后的 uniffi 绑定进 CI 会打出
「启动即 checksum mismatch」的包。**

**做法**：改了跨 IPC/FFI 边界的类型或命令签名，当场重生成对应那份并一起提交。不要指望
「下次 `tauri dev` 会自动更新」——那只在有人恰好跑 dev 的时候才发生，而 CI 从不跑 dev。

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

## 提交前 checklist

```bash
pnpm exec tsc --noEmit
cargo check --workspace
cargo fmt --all
cargo clippy --workspace -- -D warnings   # 项目期望零 warning
./scripts/check-wasm.sh                   # wasm 双 target；改了 crates/* 必跑
```

改了跨边界类型还要**重生成三份 bindings 之一**（见上一节），CI 不会替你发现。

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
# 根 Cargo.toml —— 不能写成 [profile.release]，那是桌面的（速度优先 opt-level=3）
[profile.mobile-release]
inherits = "release"
opt-level = "z"   # 包体优先
lto = "thin"
strip = "symbols"
```

**Why**：Cargo 只认 workspace root 的 profile，**member 自己的 profile 会被静默忽略**
（只有一行 warning）。mobile-core 并入前是隐式 workspace root、自带这套配置；并入后若不
搬到根，移动端包体优化就无声消失。消费方是 `ubrn build <platform> --profile mobile-release`
（ubrn 的 `-p` 覆盖 `-r`），产物落在 `target/mobile-release/` 而非 `target/release/`。
ubrn 用 `cargo metadata` 的 `target_directory` 定位产物，会自动跟到仓库根，无需额外配置。

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
- SAF 新传输用 `"wt"` 明确截断，续传用 `"rw"` 保留已有内容并做可定位写；同一进程恢复优先
  复用现有 handle，不能用 `"w"` reopen，否则 checkpoint 还在而文件已被再次截断。

补丁配置必须留在 `pnpm-workspace.yaml`（pnpm 11 不读 package.json 的 `pnpm` 字段），更新后用
`pnpm install --config.allowUnusedPatches=true` 并确认 `mobile/pnpm-lock.yaml` 出现对应
`patch_hash`。升级 expo-file-system 时先核对上游是否同时覆盖 PFD 生命周期和 SAF `"rw"` 写通道，
不能只看到 #47176 合入就直接删除整个补丁。

**相关文件**：`mobile/patches/expo-file-system@56.0.8.patch`、
`mobile/src/core/foreign-file-access.ts`、`mobile/pnpm-workspace.yaml`

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
