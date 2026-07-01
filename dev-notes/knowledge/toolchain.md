# Toolchain

## 概览

构建 / 包管理 / lint / CI 的项目特有约束。常规命令参考 CLAUDE.md "Build and Development Commands"；本主题只记非显见的坑。

## 包管理

### pnpm only

项目锁定 pnpm（`packageManager` 字段 + pnpm-lock.yaml）。npm / yarn install 会产生不一致的 lockfile。

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

**正确做法**：SwarmDrop（有 `packageManager: "pnpm@9.0.0"`）的 workflow 里 `pnpm/action-setup@v4` **不要带 `with: version`**，让它读 packageManager。RN（无 packageManager 字段）靠 action 的 `version` 指定。别混用。

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
