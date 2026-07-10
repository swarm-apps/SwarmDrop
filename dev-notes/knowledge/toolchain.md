# Toolchain

## 概览

构建 / 包管理 / lint / CI 的项目特有约束。常规命令参考 CLAUDE.md "Build and Development Commands"；本主题只记非显见的坑。

## 包管理

### pnpm only

项目锁定 pnpm 11（`packageManager` 字段 + pnpm-lock.yaml）。npm / yarn install 会产生不一致的 lockfile。

### pnpm 11 settings 放在 pnpm-workspace.yaml

pnpm 11 不再把 `package.json` 里的 `pnpm` 字段作为项目 settings 来源。需要配置 overrides、允许依赖
build scripts、release-age 策略等，都放到对应项目根的 `pnpm-workspace.yaml`。

**正确做法**：
- 根桌面应用用仓库根 `pnpm-workspace.yaml`，当前仅声明 `packages: ["."]` 和允许 `esbuild` build script。
- 独立 e2e 子项目用 `e2e/desktop/pnpm-workspace.yaml`，把 WDIO 相关 overrides 和 build-script 策略放在那里。

**不要做**：
- 在 `package.json` 里新增 `pnpm.overrides` / `pnpm.onlyBuiltDependencies`，pnpm 11 下容易被忽略或造成 lockfile 不一致。

**相关文件**：`package.json`、`pnpm-workspace.yaml`、`e2e/desktop/package.json`、`e2e/desktop/pnpm-workspace.yaml`

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
详见 [`dev-notes/blogs/desktop-webdriver-e2e.md`](../blogs/desktop-webdriver-e2e.md) 的
"常见坑"。

**最容易踩的一个坑**：native 二进制必须用 `pnpm tauri build --debug --no-bundle` 构建，裸
`cargo build` 出来的二进制会因为 `tauri.conf.json` 的 `devUrl` 指向没启动的 Vite dev server
而白屏（窗口标题读出来是空字符串，`window.__TAURI__.core.invoke` 一直超时）。

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
- `send-file.demo.ts` 允许没有在线已配对设备：此时只输出首页/空环境素材，不把录制管线判失败。
- 录制产物在 `build/` 下，仓库根 `.gitignore` 已忽略，不要提交视频原始文件。

**相关文件**：`e2e/desktop/scripts/record-desktop-demo.mjs`、`dev-notes/blogs/desktop-demo-recording-pipeline.md`

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

**相关文件**：`e2e/desktop/scripts/record-transfer-demo.mjs`、`../SwarmDrop-RN/e2e/webdriver/`

## Cargo 配置

### dev profile 所有依赖 opt-level = 3

`Cargo.toml` 根：

```toml
[profile.dev.package."*"]
opt-level = 3
```

**Why**：crypto 依赖（`tauri-plugin-stronghold` / `chacha20poly1305` / `blake3` 等）和 libp2p 不开优化会慢 10-100×，dev 体感卡顿明显。

**不要做**：删除这段配置或把 `*` 改成具体 crate 列表——会漏掉新加的 crypto/网络依赖。

### workspace members 固定 4 个

```toml
members = ["crates/core", "crates/entity", "crates/migration", "src-tauri"]
```

加新 crate 必须显式登记，否则 `cargo check --workspace` 不会覆盖。

## Vite / Tauri 联动

### 端口固定 1420，HMR 走 1421

Tauri dev 期间硬编码连这两个端口。改 `vite.config.ts` 端口会让 `pnpm tauri dev` 白屏。

**相关文件**：`vite.config.ts`

### TAURI_DEV_HOST 用于真机调试

`vite.config.ts` 读取 `TAURI_DEV_HOST` env：设了就把 host / hmr.host 切到该 IP。本地 dev 不需要设。

### /impeccable live 模式对 Tauri 原生窗口可行，别一上来就判"不支持"

第一反应容易觉得 live 模式（`.claude/skills/impeccable`）只认浏览器 tab，Tauri 原生窗口用不了——**这个判断是错的**，已经实测验证过。

**为什么能用**：live 模式的注入机制就是往 `index.html`（见 `.impeccable/live/config.json` 的 `files`）插一段 `<script>`，而 `pnpm tauri dev` 本质是 `BeforeDevCommand: pnpm dev --host` 起 Vite，Tauri 原生窗口只是加载同一个 Vite dev server 的 `index.html`。跑 `node .claude/skills/impeccable/scripts/live.mjs` 注入后，Vite 的 HMR 会让已经打开的 Tauri 窗口自动重载，注入的悬浮选取器工具条会直接出现在真实窗口里，App 本身也照常渲染（因为 Tauri IPC 上下文还在）。

**踩过的坑**：单独用 `pnpm dev`（不走 `pnpm tauri dev`）在普通浏览器 tab 里打开 `http://localhost:1420` 会是**空白页**——这是因为前端 mount 时就会 `invoke()` 走 Tauri IPC（network-store / auth-store 等），普通 Chrome tab 没有 `window.__TAURI_INTERNALS__`，直接崩渲染。所以"浏览器 tab 打开空白"和"live 注入机制在 Tauri 里失效"是两件不同的事，别混为一谈。

**agent 这边怎么驱动**：live.md 文档写的是用 `browser_navigate` 之类的浏览器工具去看/截图，这对 Tauri 不适用；改用 `mcp__tauri__driver_session`（需要项目已装 `tauri-plugin-mcp-bridge`，本仓库已装）连接同一个正在跑的原生窗口，`mcp__tauri__webview_screenshot` / `webview_execute_js` 代替浏览器截图/取值，`live-poll.mjs` 的本地 HTTP helper 完全不关心注入的 JS 跑在哪个 webview 里，所以轮询/accept/discard 那套照常工作。

**已知脆弱点**：往 `index.html` 里改动（`live.mjs` 注入、或后续手动 edit 触发 HMR 全量刷新）会让已连接的 `mcp__tauri__` bridge 掉线——`webview_execute_js` / `webview_screenshot` 会报 `WebView execution failed`，需要重新 `driver_session action:"start"`；遇到过一次重连失败，发现是 `pnpm tauri dev` 整个进程都退出了（不是 panic，像是窗口被动关闭），得重新 `pnpm tauri dev` 再连。改动 index.html 或触发大范围 HMR 刷新后，预期要重连一次 driver_session，不代表出错。

**相关文件**：`.impeccable/live/config.json`（`files: ["index.html"]`）、`src-tauri/Cargo.toml`（`tauri-plugin-mcp-bridge`）

### Tauri MCP 事件模拟：payload 必须是对象，快捷发送链路可整段模拟

用 tauri MCP 验证 UI 时两个实测经验：

**正确做法**：
- 模拟「右键用 SwarmDrop 发送」全链路不需要真实右键：emit `external-file-open` 事件（payload `{ paths: ["/abs/path", ...] }`）即可触发 ExternalOpenHandler → share-store → `/send/share-target` 完整前端链路
- emit 事件时 payload 必须是 **JSON 对象**。`mcp__tauri__ipc_emit_event` 的 payload 参数如果传了字符串化 JSON，前端 `event.payload` 收到的是 string，`payload.paths` 为 undefined，listener 静默失败、页面毫无反应。保险做法是用 `webview_execute_js` 执行 `window.__TAURI__.event.emit("external-file-open", { paths })`

**不要做**：
- 通过 `window.location.href = "/xxx"` 验证路由 redirect——整页刷新会丢内存态（解锁状态），app 会弹回 unlock 屏

**相关文件**：`src/components/external-open-handler.tsx`、`src/lib/bindings.ts`（`events.externalFileOpen`）

## Git submodule

### libs/ 是 swarm-p2p submodule

```
[submodule "libs"]
    path = libs
    url = https://github.com/yexiyue/swarm-p2p.git
```

**克隆后必须**：`git submodule update --init --recursive`，否则 `cargo build` 找不到 `swarm-p2p-core`。

**注意**：`libs/core` 使用 **Rust 2024 edition**，与主仓 2021 edition 不同——给 submodule 加新文件时注意 edition 差异（比如 `unsafe` block 在 2024 更严格）。

## Lingui 提取

### sourceLocale 是 zh，实际只有 3 个 locale

```ts
sourceLocale: "zh",
locales: ["zh", "zh-TW", "en"],
```

CLAUDE.md 顶部写"8 locales"——那是规划目标，**当前实际是 3 个**。新增 locale 前先确认设计资源就绪。

### 提取命令必须先于 commit

```bash
pnpm i18n:extract
```

漏跑会导致 `src/locales/*/messages.po` 缺少新加的字符串，运行时降级显示原文。

## 版本号同步

三处版本必须一起改（已经踩过坑）：

| 文件 | 字段 |
|---|---|
| `package.json` | `version` |
| `src-tauri/Cargo.toml` | `package.version` |
| `src-tauri/tauri.conf.json` | `version` ←  **release 真源**（CI 用这个） |

`crates/core/Cargo.toml` 的 version 也建议同步，方便 SwarmDrop-RN 共享 core 时对版本。

## 提交前 checklist

```bash
pnpm exec tsc --noEmit
cargo check --workspace
cargo fmt --all
cargo clippy --workspace -- -D warnings   # 项目期望零 warning
```

## CI / Release

两仓都由 push `v*` tag 触发 release CI（`.github/workflows/release.yml`）。发版 = bump 版本 + commit + tag + push tag。

### Tauri workspace 的 release bundle 在根 `target/`

SwarmDrop 的 `src-tauri` 是 Cargo workspace member，不是独立 Cargo 项目。`tauri-action`
打包后产物位于仓库根目录的 `target/.../release/bundle/`，不是 `src-tauri/target/...`。

**正确做法**：
- SwarmHive `artifact-paths` 同时扫 `target/${{ matrix.swarmhive_target }}/release/bundle/**/*`
  和 `target/release/bundle/**/*`。
- 不要写成 `src-tauri/target/...`，否则 `swarmhive-action@v2` 会选不到 updater bundle，
  CI 在上传步报 `no updater bundles selected`。

**相关文件**：`.github/workflows/release.yml`

### pnpm/action-setup 不能与 packageManager 双指定

`pnpm/action-setup` 的 `with: version:` 和 `package.json` 的 `packageManager` 字段**不能同时存在**，否则报 `Multiple versions of pnpm specified` / `ERR_PNPM_BAD_PM_VERSION`，CI 在 Setup pnpm 步骤直接失败。

**正确做法**：SwarmDrop（有 `packageManager: "pnpm@11.10.0"`）的 workflow 里 `pnpm/action-setup@v4` **不要带 `with: version`**，让它读 packageManager。RN（无 packageManager 字段）靠 action 的 `version` 指定。别混用。

### windows updater bundle 选取：用清单内匹配，别用 `[ -f ]`

`release.yml` 的 "Pick SwarmHive updater bundle" 从 tauri-action 的 `artifactPaths` 里挑「有同名 `.sig`」的 updater bundle。两个坑：(1) **windows 的 `D:/...` 盘符路径在 Git bash 下 `[ -f "${f}.sig" ]` 不可靠**（漏判 → updater 选空 → exit 1）；(2) tauri v2 windows updater 产物是 `-setup.exe` + `.exe.sig`（**没有 `.nsis.zip`**）。

**正确做法**：把清单 `jq -r '.[]' | sed 's#\\#/#g'` 转正斜杠存进 `paths`，用 `grep -qxF "${f}.sig" <<< "$paths"` 在清单内判断有无同名 sig；windows 优先选 `*-setup.exe`。

### 双仓 core 依赖：本地 path / 发版 git

RN 的 `packages/swarmdrop-core/rust/mobile-core/Cargo.toml` 依赖 SwarmDrop 的 `swarmdrop-core`/`entity`/`migration`/`swarm-p2p-core`：

- **本地联调**：四个都用 `path = "../../../../../SwarmDrop/crates/*"`（+ `libs/core`），需平级 checkout。
- **发版**：四个都换成 `git = "https://github.com/swarm-apps/SwarmDrop.git", tag = "vX.Y.Z"`。

**不要做**：git 与 path 混用——`swarm-p2p-core` 会撞 `multiple versions`，四个必须统一来源。cargo 拉 git 依赖会自动递归拉 SwarmDrop 的 `libs` submodule（swarm-p2p）。

**发版顺序**：先发 SwarmDrop（bump + tag `vX.Y.Z` + push tag），tag 在远端存在后，再把 RN 的 git 依赖 pin 到该 tag、发 RN，保证 RN core pin 到可复现的 release commit。

**相关文件**：`.github/workflows/release.yml`（两仓）、`SwarmDrop-RN/packages/swarmdrop-core/rust/mobile-core/Cargo.toml`
