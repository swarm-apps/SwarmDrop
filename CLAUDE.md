# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

> **本文件是本仓架构描述的唯一事实源。** `AGENTS.md` 只是指向这里的入口。改动架构后请同步本文件——
> 历史上这份文档曾漂移出整整一个大版本（描述 libs/ submodule、Stronghold、6 位分享码），
> 严重误导后续开发。

## 开发工作流

**IMPORTANT**：执行任何开发任务（编写代码、修改配置、添加依赖）前，必须先调用 `/dev-workflow` skill。它会加载项目知识库（`dev-notes/knowledge/`）中的最佳实践和踩坑记录，并在开发完成后引导更新知识库。

知识库主题：

- [`dev-notes/knowledge/theme-and-styling.md`](dev-notes/knowledge/theme-and-styling.md) — shadcn/ui、Tailwind、macOS Overlay 标题栏、Zustand selector 派生数组陷阱、Lingui 源 locale
- [`dev-notes/knowledge/rust-backend.md`](dev-notes/knowledge/rust-backend.md) — crates/core ↔ src-tauri 边界、specta + chrono、`#[expect]` 风格、IPC 时间类型选型
- [`dev-notes/knowledge/toolchain.md`](dev-notes/knowledge/toolchain.md) — Cargo dev profile opt-level、`mobile-release` 的 blake3 单包例外、Vite/Tauri 端口、Lingui 实际 locale、版本号三处同步、**pnpm patch 打在有预编译产物的原生依赖上会静默失效**
- [`dev-notes/knowledge/net-kernel.md`](dev-notes/knowledge/net-kernel.md) — 网络内核 swarmdrop-net（2026-07 重构产物）：架构速览与事件双轨制、libp2p git pin 校准坑、wasm 工程约定、wire v2 契约点、已知负债。**碰 crates/net、crates/net-base、协议注册、relay、DHT、升级 libp2p rev 时必读**
- [`dev-notes/knowledge/libp2p-wasm.md`](dev-notes/knowledge/libp2p-wasm.md) — Web 端（wasm）可行性调研（2026-07）。**结论已落地**：`crates/web` + `docs/app/app` 是其产物
- [`dev-notes/knowledge/web-app-frontend.md`](dev-notes/knowledge/web-app-frontend.md) — Web 应用区**表现层**（`docs/app/app`）：运行时单例只挂 layout、静态导出三限制（无 redirect / 无动态段 / useSearchParams 要 Suspense）、basePath 与 next/link、zustand store 的 selector 与 `setState` 约束。**碰 Web 端 React 代码时必读**
- [`dev-notes/knowledge/storage-abstraction.md`](dev-notes/knowledge/storage-abstraction.md) — 把 sea-orm 从 core 摘出去。**已落地**：core 零 sea-orm，SQL 实现在 `crates/storage-sql`，Web 端是 IndexedDB 写穿的 `WebTransferStore`（`crates/web/src/store.rs` + `inbox.rs`）。另含端口体例：`SessionStore` / `InboxStore` 均已补全、收件箱领域规则住 `crates/transfer/src/inbox.rs` 由各存储实现调用、组装点建一次端口 `Arc` 注入与自持同一份
- [`dev-notes/knowledge/iroh-migration.md`](dev-notes/knowledge/iroh-migration.md) — libp2p → iroh 迁移评估（2026-07 调研）。**已决策：不迁移**，但 iroh 的 API 形态被 `crates/net` 借鉴。碰 P2P 选型或有人提「迁 iroh」时先读
- [`dev-notes/knowledge/app-update.md`](dev-notes/knowledge/app-update.md) — 应用内更新（SwarmHive）：`ready` 是持久静止态而非「正在等系统」、Android 10+ 后台安装框弹不出且**静默**失败、自动安装必须单点触发、状态判据要穷尽 8 态、续传与产物恢复。**碰更新 UI、`@swarm-hive/sdk`、两个 registry 分发的文件时必读**

## Design Context

做任何 UI/视觉相关工作前，先读根目录的 `PRODUCT.md`（产品定位、目标用户、反面参考、设计原则）和 `DESIGN.md`（配色/字体/组件/elevation 等视觉令牌，含 `.impeccable/design.json` sidecar）。两者由 `/impeccable` 系列命令维护。

## Language

Always respond in Chinese (简体中文). All output, including thinking, planning, commit messages, and comments, must be in Chinese.

## Project Overview

SwarmDrop is a decentralized, cross-network, end-to-end encrypted file transfer tool built with Tauri v2. It aims to be a "cross-network version of LocalSend" — no accounts, no servers, supporting both LAN and cross-network peer-to-peer file transfers.

**Current Status:** 桌面 / 移动 / Web 三端。桌面与移动已发布，Web 端（wasm）随文档站部署到
GitHub Pages（Phase 5 仍在收敛，见下方 Development Phases）。当前重心已从「把 Web 端跑通」
转到**三端传输链路的真机收敛**——吞吐、续传基线、接收落点。
Current desktop release: **v0.15.2**（bootstrap 独立版本线，当前 `bootstrap-v0.7.2`；移动 `mobile-v0.15.2`）。

## Build and Development Commands

```bash
# Full app development (Vite frontend + Tauri Rust backend)
pnpm tauri dev

# Frontend only (Vite dev server at http://localhost:1420)
pnpm dev

# Production build
pnpm build              # Frontend (tsc + vite build)
pnpm tauri build        # Full app

# 前端单测（vitest）
pnpm test

# Zustand 两条规则：A 禁止绕过 selector 直接 getState/setState（仅 src/）；
# B 禁止 selector 里派生新数组/对象（src/ 与 docs/app/app 都扫）
pnpm check:zustand-access

# 共享包零平台依赖（两道门：import 纯度 + 无 DOM lib 的 tsc）
pnpm check:shared-view

# 禁止绕过 src/lib/clipboard.ts 直接用 navigator.clipboard
pnpm check:clipboard

# 配对落地页（docs/public/p/）体积 ≤10KB gzip（注释也算）+ en/zh-TW 字典完整性
pnpm check:landing

# Rust（在仓库根目录跑，workspace 一并覆盖）
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace
cargo fmt --all

# wasm 双 target 门禁（CI 必过；覆盖 net-base/net/host/transfer/invite/core/web）
./scripts/check-wasm.sh
./scripts/check-wasm.sh --clippy

# crates/web 的 wasm 测试（headless Chrome；上面两条只保证「编得过」）
# 自动取与本机 Chrome 主版本匹配的 chromedriver —— 版本错位会以一句 404 失败，
# 看起来跟测试挂了一模一样，见 toolchain.md
./scripts/test-wasm.sh

# i18n — 提取前端翻译串到 .po
pnpm i18n:extract

# Changelog（排除 mobile/，只认 v* tag）
pnpm changelog

# 文档站 + Web 端（在 docs/ 下跑 —— 独立 pnpm workspace）
pnpm dev                # Next.js dev server
pnpm build:wasm         # wasm-pack build crates/web → packages/swarmdrop-web
pnpm build
pnpm test               # vitest。**根 vitest 显式 exclude 了 docs/**，只有这条能跑到它们
pnpm typecheck          # fumadocs-mdx + lingui compile + next typegen + tsc

# 桌面 e2e / 录屏（在 e2e/desktop/ 下跑 —— 独立 pnpm workspace）
pnpm wdio

# Mobile（在 mobile/ 下跑 —— 独立 pnpm workspace）
pnpm install
pnpm ios / pnpm android
pnpm typecheck
pnpm --filter react-native-swarmdrop-core build:ios      # 重建 uniffi 桥接
```

**Package manager:** pnpm only (not npm or yarn).

**No git submodules.** 旧的 `libs/`（swarm-p2p-core）已于 2026-07 删除，`.gitmodules` 不存在——
克隆后直接 `pnpm install` 即可。网络栈由自研的 `crates/net` 取代（权威的 swarm-p2p 历史源在
独立仓 `swarm-apps/swarm-p2p`，本仓不再依赖）。

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Frontend | React 19, TypeScript 5.8, Vite 7, Tailwind CSS 4 |
| Routing | TanStack Router (file-system based, auto code-splitting) |
| State | Zustand 5 |
| UI | shadcn/ui (new-york style), Lucide icons, Radix primitives |
| i18n | **三端同为 Lingui 6**（桌面 Babel macro · Web SWC plugin · 移动 Metro transformer）。桌面与 Web 同一组 locale（zh / zh-TW / en），移动是 zh-Hans / en；**四份独立 catalog**——三份前端 Lingui + 后端 rust-i18n（托盘与系统通知等原生串，`pnpm i18n:extract` **扫不到**它） |
| IPC | tauri-specta v2 —— TS bindings 自动生成，**不手写 invoke 封装** |
| Backend | Rust 2024, Tauri 2 |
| P2P | 自研 `crates/net`（iroh 风格 API，libp2p 底层，native + wasm 双 target） |
| Security | Ed25519 身份 · 系统钥匙串（keyring 4）· Noise/TLS 传输层加密 · BLAKE3 + bao-tree 逐块验签 |
| Database | SeaORM 2.0 + SQLite（传输历史、断点续传 checkpoint、收件箱）——**仅 native** |
| MCP | rmcp 2 + axum（桌面本地 MCP server） |

> **Stronghold 已移除。** 私钥现由宿主 keychain 端口管理（桌面 = `keyring` 系统钥匙串），
> 前端 `secret-store` 只是运行时镜像，不再持久化任何密钥。
>
> **生物识别已移除（2026-07-27）。** 它随密码解锁流程一起失效后，依赖、插件注册和
> capability 又空挂了一段时间，期间 README 与文档站一直在宣传一个不存在的功能。
> 现已清理干净（`tauri-plugin-biometry`、`@choochmeque/tauri-plugin-biometry-api`、
> `setup.rs` 的 plugin 注册、`biometry:default` 权限）。若将来要做，需重新接线。
>
> **不要再加应用层加密。** 传输的保密由传输层承担（Noise / QUIC-TLS，relay 只见密文）；
> 应用层 XChaCha20-Poly1305 已在 wire v2 **整块删除**，生产代码零加密原语。它是自引用的
> 冗余——密钥经同一条 Noise 信道分发，能读密文的攻击者必然也能读密钥；且它与 bao-tree
> 逐块验签**不能共存**（加密后 `checksum` 会变成密文哈希，「root == 明文 blake3」这条
> 不变量就塌了）。删除后归属校验由传输层身份补上：数据面必须校验
> `stream.remote() == session.peer`。完整推导见
> [`blogs/transfer-architecture/05-removing-encryption-layer.md`](dev-notes/blogs/transfer-architecture/05-removing-encryption-layer.md)。
>
> **发送准备只读一遍源文件，且 bao chunk group == `CHUNK_SIZE`（2026-08-09）。**
> `prepare` 一遍流式读同时产出 `checksum` 与验签树——`build_outboard_from_source` 返回的
> root **就是** checksum，不是「另算一遍再断言相等」（此前是后者，靠 release 下不执行的
> `debug_assert_eq!` 兜着，而四处文档已经宣称它是一遍了）。进度经 `bao::ReadProgress` 挂在
> 那唯一一遍的 reader 上，因此**覆盖全部真实工作量**，不再有「进度条走完后再静默等一倍
> 时间」。三条推论不能破：
> ①**改 `bao::BLOCK_SIZE` 等于改 wire**（proof 树形状变，旧端第一个块就验签失败），必须
> 同时 bump `TRANSFER_DATA_PROTOCOL`——数据面协议名同时承载「帧怎么编码」与「验签树什么
> 形状」，判据写在 `openspec/specs/transfer-data-plane`；
> ②持久化 outboard 的失效判据是**长度**（`bao::is_outboard_usable`）不是「是否为空」——
> 格式作废的存量 BLOB 非空且看起来合法，用 `is_empty()` 判会让那条会话**永久**续不上传
> 且不报错；
> ③「bao 顺序读」是 bao-tree 的**实现事实而非契约**，进度单调性与读取等长判据都依赖它，
> 由 `records_sequential_forward_reads` 那条护栏测试独家看守。
> 推导见 [`blogs/transfer/02-chunk-group-realignment.md`](dev-notes/blogs/transfer/02-chunk-group-realignment.md)
> 与 `openspec/changes/single-pass-prepare/`。
>
> **接收是「暂存 → 发布」两段，且 finalize 只发布不校验（2026-08-07）。** 数据块先随机写进
> 一个**本进程完全拥有**的暂存位置（桌面 `<dst>/x.part`、移动 `<data_dir>/staging/`），
> 单个文件**收齐即发布**到用户目标位置。两条不变量不能破：
> ①「DB 里 bitmap 完整 ⟺ 该文件已发布」——所以末块**不刷** checkpoint，完整 bitmap 只由
> 发布成功后的 `mark_file_completed` 写；②finalize 失败只意味着「数据是好的、只是搬不过去」，
> **不得 reset checkpoint**（会让对端重传整个文件），直接上抛走可恢复的 Interrupted。
> 移动端的接收写盘因此**整条在 Rust 侧**，`ForeignFileAccess` 只剩读发送源与 SAF 相关的三件事。
>
> ⚠️ **两段式要留着，但它当初的归因是错的（2026-08-10 更正）。** 这里此前写作「根因是
> SAF 的 fd 不归本进程所有、`lseek` 必 `EBADF`」——那是误诊。真根因是 `expo-file-system`
> 的 `forContentURI` 不持有 `ParcelFileDescriptor`，GC finalizer 把 fd 关了；**而本仓修这条
> 的 pnpm patch 从未进过 Android 构建**（SDK 56 默认吃预编译 AAR），于是「改了三次都没修好」
> 被反推成「SAF 的 fd 天生不能用」。两段式的理由换成与 fd bug 无关的这四条：SAF/FUSE 上随机
> 写慢、用户目录不该出现半成品、暂存要跨「中断 → 过几天再恢复」存活、**部分 DocumentsProvider
> 返回不可 seek 的 fd**（管道式 `openDocument`，`position()` 一律失败）——最后一条也是
> 「发布只做顺序写、绝不 `setOffset`」这条规则**独立成立**的理由。
> 完整推导见 [`knowledge/rust-backend.md`](dev-notes/knowledge/rust-backend.md)、
> [`knowledge/toolchain.md`](dev-notes/knowledge/toolchain.md)（patch 为什么没生效）与
> `openspec/changes/receive-staging-publish/`。
>
> **接收落点恒为用户可见位置，且与应用私有数据区分离（2026-08-09）。** 移动端曾把两者放在
> 同一个 `Paths.document.uri` 下（库 + staging + 收到的文件），落点因此继承了「私有」属性：
> Android 的 SAF 看不见它、iOS 没开文件共享也看不见它——收到的文件既在文件管理器里找不到，
> 也无法在发送页被选中转发（发送侧四个来源全是系统 picker）。现在两个角色分开：
> `getPrivateDataDir()`（iOS = `Library/Application Support/`，Android = `<internal>/files/`）
> 装库与 staging；`@/core/receive-location` 管用户可见接收区（iOS = `Documents`，经
> `UIFileSharingEnabled` 暴露；Android = 用户在引导流程里选定的 SAF tree）。
> **没有私有目录回退**——落点是 `ready` / `unconfigured` / `revoked` 三态，未就绪时接收被拦。
> 副产品是 `save_dir` 的第三态消失：iOS 恒 `file://`、Android 恒 `content://`，两条 publish
> 路径各自对应一个平台。判据与三端形态写在 `DESIGN.md` 的 **Receive Location Contract**，
> 「从收件箱转发」写在 **Received File Reuse Contract**。见
> `openspec/changes/visible-receive-location/`。

## Architecture

### Workspace 布局（Cargo）

根 workspace 有 12 个 crate + 桌面壳 + 移动桥接。分层自下而上：

> **JS 侧另有 `packages/`**（不进 Cargo workspace）：`packages/shared-view` 是三端共享的纯视图
> 逻辑（TS 源，零依赖），`packages/file-browser` 是桌面与 Web 共享的**文件浏览器组件**
> （React DOM；移动端不吃它，RN 与 DOM 不共用 JSX），`packages/swarmdrop-web` 是
> `crates/web` 的 wasm 产物（入库）。
> `docs/` 与 `mobile/` 各自用 `link:` 引用它们——两者是独立 pnpm workspace，不是这里的成员。

| Crate | 职责 |
|---|---|
| `crates/net-base` | 网络类型底座。`NodeId` / `Addr` / `NodeAddr` / `ProtocolId` / `NatStatus` —— libp2p 类型在此收口成 newtype，**不向上穿透** |
| `crates/net` | 网络内核 `swarmdrop-net`。iroh 风格 `Endpoint` 门面 + 后台 actor，隐藏事件循环、连接管理、协议路由、地址选择 |
| `crates/webrtc-p2p` | libp2p WebRTC 传输，**两种模式**：打洞（`/webrtc`，spec `/webrtc-signaling/0.0.1`，三端默认开启——打洞要两端都支持，只开一边等于没开）+ direct（`/webrtc-direct`，**已完全取代官方 `libp2p-webrtc` 与 `libp2p-webrtc-websys`**，native 监听 + 拨号、浏览器拨号均已实测跑通）。刻意不带 swarmdrop 前缀、不依赖任何 swarmdrop crate，将来要 subtree split 出去独立发布 |
| `crates/host` | 宿主端口层（platform-neutral ports + DTO + error + device 类型），供 core 与 transfer 共同依赖。现有 6 个端口：`KeychainProvider` / `PairedDeviceStore` / `DeviceConfig` / `FileAccess` / `Notifier` / `UpdateInstaller`（`AppPaths` 已删，零实现零消费）。设备名归一化的唯一入口 `DeviceName::parse` 也在这里 |
| `crates/invite` | PairInvite 编解码 + 一次性状态表 + 二维码。**wasm-clean，不依赖 core** |
| `crates/transfer` | 文件传输域 + 收件箱领域模型（`inbox.rs` 的 DTO 与共享规则，各存储实现调它）。经端口 trait 依赖倒置，**不依赖 sea-orm / pairing / network** |
| `crates/core` | 平台无关业务核心：identity / network / pairing / presence / device_manager / protocol / infra |
| `crates/storage-sql` | `SessionStore` / `InboxStore` 端口的 SeaORM+SQLite 实现，**native-only** |
| `crates/entity` | SeaORM entity。sea-orm 已 feature 解绑（Web 端可只吃类型宏） |
| `crates/migration` | SeaORM migration |
| `crates/web` | 浏览器 Web 壳。除 `types` 外全部 `cfg(wasm_browser)` 门控 |
| `crates/bootstrap` | 公网引导 + relay 节点（复用同一个 `Endpoint`，不与客户端内核分叉） |
| `src-tauri` | 桌面壳 |
| `mobile/packages/swarmdrop-core/rust/mobile-core` | uniffi 移动桥接 |

**关键边界：** `crates/core` 零 sea-orm、`crates/transfer` **零 core 依赖**、`crates/invite` 零 core 依赖。

> 这里曾写作「`crates/transfer` 零 network 依赖」，**是错的**（2026-08-05 核实修正）：
> transfer 的 `Cargo.toml:21` 就有 `swarmdrop-net`，`protocol.rs` / `incoming.rs` /
> `manager.rs` 直接用 `Endpoint` / `RpcService` / `NodeId` —— 它要收发数据，不可能不认识网络。
> 真正成立的那条是**零 core 依赖**（`grep swarmdrop_core crates/transfer/src` 零命中），
> 它依赖的是 net + host + entity。
这不是审美，是 wasm 能编过的硬约束——破坏它 `./scripts/check-wasm.sh` 会红。

### Frontend → Backend Communication

**不要手写 `invoke` 封装。** IPC 走 tauri-specta 自动生成的 `src/lib/bindings.ts`：

```typescript
import { commands, events, type PairedDeviceInfo } from "@/lib/bindings";

await commands.start(pairedDevices);
```

`pnpm tauri dev`（debug build）启动时自动重新导出 bindings.ts——改了 Rust 端 IPC 类型
**不要手动改 bindings.ts**。命令与事件在 `src-tauri/src/setup.rs` 的
`collect_commands![]` / `collect_events![]` 里集中注册。

> 旧文档提到的 `src/commands/`（手写 invoke 封装）**已不存在**，不要去找它。

Rust 命令薄壳在 `src-tauri/src/commands/`，按业务域分文件：`lifecycle` / `inbox` / `identity` /
`pairing` / `transfer` / `mcp` / `i18n` / `external_open`。它们**不持有业务逻辑**，只解析参数、
取 State、调 `swarmdrop_core` 的 manager。

新增 Tauri 命令时用 `/edgemind-tauri-command` skill，并查 `/tauri-specta`。

### Frontend Architecture

**Routing** — TanStack Router，文件系统约定在 `src/routes/`：

- `__root.tsx` — 根布局
- `_onboarding.tsx` + `_onboarding/device-name.lazy.tsx` — 首启引导（**只有设备命名**）
- `_app.tsx` — 主布局（`AppTopBar` + 内容区。**所有**路由都挂顶栏，无全屏例外：
  `/pairing` 曾经是全屏路由、自绘一条只有窗口按钮的玻璃条，那条 strip 与页面自己的
  `TaskToolbar` 叠成两层头，Windows 上尤其像没做完。`AppTopBar` 本身就带
  `data-tauri-drag-region` 与 `WindowControls`，无边框窗口需要的拖拽/控制它全包了）
- `_app/devices/` — 设备（含 `-components/` 局部组件、trust policy、分组）
- `_app/send/` — 发送（含 `share-target` 外部打开入口）
- `_app/inbox/` — 收件箱
- `_app/transfer/` — 传输列表 + `$sessionId` 详情
- `_app/pairing/` — `generate` / `input` 两条配对路径
- `_app/settings/` — 设置（分 section 文件：网络 / 传输 / MCP / bootstrap / 设备信息 / 关于）
- `index.tsx` — 重定向

> **密码 / 解锁 / 生物识别登录流程已整体移除。** 不再有 `_auth` 布局、`auth-store`
> 或 `isSetupComplete` 守卫。首启只问设备名，身份由后端 keychain 静默管理。

**State Management** — Zustand stores：

| Store | 用途 | 持久化 |
|---|---|---|
| `preferences-store` | 主题、语言、设备名 | `tauri-plugin-store`；`onRehydrateStorage` 立即应用避免闪烁 |
| `secret-store` | 设备身份 + 已配对设备的运行时镜像 | 无（每次启动从 `initializeIdentity()` 读） |
| `network-store` | 节点状态、peer map、监听地址、NAT | 运行时 |
| `pairing-store` | 配对流程状态 | 运行时 |
| `transfer-store` | 活动会话、待决 offer、进度 | 运行时 |
| `inbox-store` | 收件箱列表与搜索 | 运行时 |
| `share-store` | 待发送文件选择（含 share-target 注入） | 运行时 |

**Zustand 访问约束**：selector 里禁止派生新数组/对象（会无限重渲染），除非包 `useShallow`。
`pnpm check:zustand-access` 是机器兜底（两条规则，覆盖 `src/` 与 `docs/app/app`），
细节见 `dev-notes/knowledge/theme-and-styling.md` 与 `zustand-store-usage.md`。

**Responsive Design** — 桌面端**没有侧边栏，也没有底部导航**：全局导航是 `AppTopBar`
的顶栏 + 面包屑（`src/components/layout/app-topbar.tsx`），导航深度靠面包屑表达。
这是 `DESIGN.md` 里的刻意简化，不是待补的缺口——加 nav rail 前先读那里第 204 / 228 行。

页面级主从布局用 `MasterDetailShell`（收件箱 / 传输活动 / share-target 共用），
单一断点 `MASTER_DETAIL_QUERY = (min-width: 920px)`：≥920 左列表 + 右详情双栏，
<920 详情占满、列表从左抽屉滑出。**所有 master-detail 页都用这一个断点，不要各页写各的。**
其余响应式靠 Tailwind 断点类。hook 是 `src/hooks/use-media-query.ts` 的 `useIsWideLayout`。

920 这个数不止 `MasterDetailShell` 用：设备页与两个配对页的主分栏也写 `min-[920px]:`。
**任何「主内容 + 侧栏说明」的分栏都用它，别退回 `lg:`（1024）**——Windows 常见的 125%
缩放下 1200 物理像素只有 960 CSS 宽，正好落在 920 与 1024 之间，于是同一个窗口里
设备页分栏、配对页却堆叠（配对页此前就是这么不一致的）。

窄屏空态的分工同理有一条约定：教学文案（「怎么让它变得非空」）放**详情侧**的
`CenteredEmptyState`，列表栏只用 `RailEmptyHint` 说一行「这里是空的」。因为窄屏用户
落在详情屏、列表收在抽屉里；两边都摆整套空态则是宽屏下同一句话说两遍。

> Web 应用区（`docs/app/app`）是**另一套形态**——持久侧边栏 + 多路由，与桌面端有意分叉
> （#88/#90 已落地，决策与理由写在 `DESIGN.md` 的「Navigation — Web app area」）。
> 别把两边的导航描述混用。

**i18n** — 前端 Lingui 6 + Babel macro，源 locale `zh`，实际 locale 为 **zh / zh-TW / en**
（ja/ko/es/fr/de 是路线图，尚未添加）。`pnpm i18n:extract` 提取，catalog 在
`src/locales/{locale}/messages.po`。**`lingui.config.ts` 的 `include` 还含
`packages/file-browser/src`**——共享组件自带 `<Trans>`，同一句话在桌面与 Web 的 catalog
里各存一份（三端 catalog 独立这条既定约定的必然结果）。
后端原生串（托盘、系统通知）走 rust-i18n，catalog 在 `src-tauri/locales/*.toml`，
由 `set_locale` 命令跟随前端偏好切换。两者不重叠。

### Backend Architecture

`src-tauri` 是纯桌面壳，业务逻辑全在 `crates/*`。

```
src-tauri/src/
├── lib.rs              # 模块声明 + rust_i18n 初始化 + run()（含 macOS RunEvent::Opened）
├── setup.rs            # Tauri Builder 装配：plugins / specta_builder / setup hook / tracing
├── main.rs             # Binary entry point
├── commands.rs         # 命令薄壳入口 + with_manager! 宏
├── commands/           # lifecycle / inbox / identity / pairing / transfer / mcp / i18n / external_open
├── host/               # Desktop adapter：keychain(keyring) / file_keychain / notifier / paths
│                       #   / update_installer / event_bus / file_source / file_sink / device_config
├── network.rs          # NetManager 类型别名 + Tauri 事件转发
├── database.rs         # SeaORM 连接初始化 + `TransferStoreState` 类型别名 + 启动清理
├── mcp/                # 桌面 MCP server（rmcp + axum）：server / tools / resources
├── external_open.rs    # macOS Open With / Windows·Linux argv → share-target
├── tray.rs             # 托盘
├── i18n.rs             # 后端 locale 状态
├── device.rs           # 桌面设备信息
├── events.rs           # tauri-specta Event 类型
└── error.rs            # AppError (thiserror) + AppResult
```

> 传输实现已整体迁出到 `crates/transfer`；`src-tauri` 不再含 `transfer/`、`file_source/`、
> `file_sink/` 顶层模块（后两者作为 host adapter 保留在 `host/` 下）。

**Network startup flow:**

1. `commands::start()` 组装 `NetworkRuntimeConfig` 并经 core 的组合根启动
2. `swarmdrop_net::Endpoint::builder().preset(...).bind()` 起内核（事件循环封在 actor 内，
   **上层拿不到 EventReceiver**——这是与旧栈最大的心智差异）
3. `NetManager` 持 `Endpoint` + `PairingManager` + `DeviceManager`，存入 Tauri state
4. `PresenceSupervisor` / `InfraSupervisor` 由 core 自治拉起，host 层无需调用
5. 事件经 tauri-specta typed events 转发前端

**Tracing:** 默认 filter `swarmdrop=debug,swarmdrop_net=debug`，`RUST_LOG` 可覆盖。

**Bootstrap / relay node:** 自建，`47.115.172.218`——TCP 4001、QUIC 4001、
**WebRTC Direct 4003**（后者是浏览器唯一入口：https 页面拨公网裸 IP 的 `ws://` 会被
mixed content 拦，`wss://` 又要域名 + CA）。

客户端清单按端分两份，各自只列本端用得上的 transport（部署配置，不属于 P2P 内核）：
`src/lib/bootstrap-nodes.ts` + `mobile/src/core/bootstrap-nodes.ts`（原生端：tcp + quic）、
`docs/app/app/_lib/relay-helpers.ts`（浏览器：webrtc-direct）。

> **WebSocket 已于 2026-07-28 整体移除**（客户端 transport、桌面 listener、bootstrap 的
> 4002 端口）。它唯一的活是「同网浏览器直连桌面」，webrtc-direct 把这件事做得更好：
> 不占 TCP 端口、私网公网同一条路径，也没有「circuit 地址被 ws 按前缀抢走」这类误匹配
> （实测踩过，见 `crates/net/src/transport.rs` 的 `relay_first_webrtc`）。
> 副产品：Android 与桌面的 transport 栈终于一致（Android 曾因 JNI DNS 问题编不进 ws）。

**配对：PairInvite（一次性签名邀请）。**
6 位数字分享码已废弃。现在是自包含邀请串 `sd:…`（Ed25519 签名 + 128bit capability +
TTL 24h + 一次性消费）；链接走 Base64URL，二维码走同一 wire 的 `SD…` Base32 表现。
**24 小时（`INVITE_TTL_SECS = 86_400`）不是笔误**：邀请跨重启存活（openspec: invite-persistence），
所以三端都必须提供「已发出的邀请」清单与撤销入口，位置统一贴着生成入口（不进设置页）。
签名尾置以覆盖版本判别码防降级。
实现在 `crates/invite`，设计见 `openspec/changes/pair-invite-protocol/design.md`。

**DHT 的用途已变**：不再用于分享码查找，改为已配对设备的 **presence 在线记录**
（`crates/core/src/presence/`）。

### Web 端（wasm）

`crates/web` 编成 wasm 后由文档站承载，入口 `docs/app/app`。走完整 `NetManager` + 3 协议，
配对经 `pair_with_invite` 真 capability 握手；持久化是「内存读缓存 + IndexedDB 写穿」的
`WebTransferStore` = `SessionStore`（会话表）+ **`InboxStore`（独立的 `inbox` 表）**，
外加 OPFS 落盘（不吃 storage-sql），收件箱、传输历史与接收侧续传上下文跨刷新存活。
浏览器侧传输依赖 WebRTC-Direct + relay circuit（+ WebRTC 打洞，见下）。

**收件箱是真表，不是「已完成接收会话」的投影。** 会话表有 `HISTORY_CAP = 100` 淘汰，
收件箱条目**不参与**——「清空传输历史不动收件箱」这条三端不变量在浏览器上才成立。
加 object store 要同改三处（store 常量 / `DB_VERSION` / `idb.rs` 的 `STORES` 表），
漏后两处只在运行时报错。**换一张已有表的记录格式同样要提 `DB_VERSION`**，并把 `STORES`
里那个 store 的格式版本一起改——结构没变、只有字段含义变时，旧行会**成功**反序列化，
坏得无声无息。两件事都见
[`web-app-frontend.md`](dev-notes/knowledge/web-app-frontend.md)。
schema 变更**直接换，不写迁移 / 回填 / 双写**（Web 端还没有真实用户）。

**只有接收方向能续传**：浏览器无法在用户不重新选择的前提下再读同一个 `File`，
因此非终态发送会话与待决 offer 一律不落库。

wasm 是 CI 一等公民：`./scripts/check-wasm.sh` 在 PR 阶段拦截破坏浏览器 target 的改动。

**前端形态（#90 起）：持久侧边栏 + 五条路由**，分区对齐桌面端但导航形态有意分叉：

| 路由 | 常驻导航 | 内容 |
|---|---|---|
| `/app` | — | 客户端重定向到 `/app/devices`（静态导出没有服务端，`redirect()` 用不了） |
| `/app/devices` | ✅ | 已配对设备 + 活跃传输 + 配对（应用首页） |
| `/app/send` | 子页面 | 发送，接受 `?peerId=` 预选目标 |
| `/app/inbox` | ✅ | 待处理入站请求 + 已接收文件 |
| `/app/transfer` | 子页面 | 传输会话，选中态走 `?session=` |
| `/app/settings` | ✅ | 节点身份 · helper 连接 · 事件日志 |

**常驻导航只有三项**（设备 / 收件箱 / 设置），与移动端 tab 同项同序（2026-08-05）。发送与传输
是**设备的子页面**（`parent: "devices"`），从设备页进入、侧栏在它们上面高亮「设备」、页头带一条
返回链接。另外两端本来就是这个形状：桌面顶栏没有「发送」，移动端 tab 也没有发送与传输。
「发送」尤其不该进导航——DESIGN.md 的 **Send Entry Contract** 写死了「Sending starts from a
device」，常驻入口只会把用户领到那条本用于纠错的目标选择器上。

导航项定义在 `docs/app/app/_lib/nav.ts`（**单一事实源**，标题/描述/图标/徽标/父子关系都从它派生；
给某项加 `parent` 就等于把它移出常驻导航）。

**节点状态徽章可点**，弹出节点状态弹窗（`node-status-dialog.tsx`）：状态 / 运行时长 / 已配对与
在线数 / 中继，诊断（节点 ID、circuit 可达地址、身份存放位置）折叠，并提供**启停节点**。
三端同一件事的第三份实现（桌面 `NodeStatusSheet`、移动 `NodeControlSheet`）——**这三份必须
遵守的信息分层、状态语义与文案，写在 `DESIGN.md` 的 `### Node Status Contract (cross-platform)`**。
> 此处此前写作「信息分层一致」，**是错的**（2026-08-08 核实修正）：桌面那份当时按
> `windowHeight >= 700` 门控七处信息位，矮窗口下中继状态、引导节点、公网地址、监听地址
> 整片消失。那正是「先写断言、实现没跟上」的产物——所以判据现在住在契约里，且与实现同 PR 合入。

启停编排收在 `_lib/node-lifecycle.ts`——`WebNodeBootstrap` 与弹窗共用同一套启动序列。

**底座与形态**（2026-08 的 `web-ux-alignment` 起）：组件走 **shadcn/ui**（不再手写原生元素），
token 经 `@theme inline` 映射层从 fumadocs 的 `--color-fd-*` 接过来；**移动优先**，
`(min-width: 920px)` 是全应用唯一的主从断点、与桌面 `MASTER_DETAIL_QUERY` 同一个数；
文案全量走 **Lingui 6**（SWC plugin，locale 客户端选择 + localStorage 持久化）。
设备呈现遵守 `DESIGN.md` 的 **Device Card Contract**（8 项信息位，三端同一份契约）。
细节与踩坑见 [`web-app-frontend.md`](dev-notes/knowledge/web-app-frontend.md)。

四条硬约束，改这块前必读：

1. **运行时单例只挂 layout**。`WebNodeBootstrap` 一个组件里同时做 spawn 节点、
   `startEventConsumption`、`startStatePoll`、`ensureConfiguredRelays`。下放到任何 page
   就会变成每路由一份，同一事件被处理多次。它也**刻意不在 cleanup 里 `closeNode()`**（StrictMode）。
2. **静态导出三条限制**：不能用动态路由段（sessionId 是运行时 UUID，`generateStaticParams`
   预生成不出来，故传输详情用 query param）；内部导航必须走 `next/link`（手写 `<a href>`
   不加 basePath，GitHub Pages 子路径下全 404）；`useSearchParams()` 必须套 `<Suspense>`，
   否则 `next build` 报 CSR bailout。
3. **`_lib/store.ts` 用 zustand**（2026-08 从自研 `create-store.ts` 迁入，调用面几乎没动）。
   selector 一律只返回原始值或 store 内的稳定引用，派生放 `useMemo`；
   `pnpm check:zustand-access` 的规则 B 覆盖这里。另有一条 zustand 特有的约定：
   **「内容没变」要 `return s` 而不是 `return {}`**——后者是新对象，`Object.is` 判不等，
   照样广播一轮。持久化偏好在**另一个 store**（`_lib/preferences-store.ts`，localStorage），
   与运行时状态分开。
4. **翻译宏只在组件里展开**。`_lib/` 下的标签映射与纯函数一律存 `msg`` ` 描述符、由组件
   `t(...)` 展开；格式化函数**不许把 UI 占位烤进返回值**（`formatTransferRate` 算不出来返回
   `null`，「等待数据」由调用点给）。`export const metadata` 在构建期求值，只能是源 locale
   ——那是静态导出的正确行为，不是漏翻。

### Mobile (`mobile/`)

移动端（iOS / Android，React Native + Expo + uniffi）曾是独立的 `swarm-apps/SwarmDrop-RN` 仓，
现已并入本仓 `mobile/`（该仓已归档只读）。

- **Rust 桥接** `mobile/packages/swarmdrop-core/rust/mobile-core` 是根 Cargo workspace 的
  member，以 path 依赖引用 `crates/*`。改 core 立刻对移动端生效，`cargo check --workspace` 一并覆盖。
- **JS 侧**是独立 pnpm workspace（同 `docs/`、`e2e/desktop`、`video/`）——在 `mobile/` 下跑
  `pnpm install` / `pnpm typecheck`，根 workspace 的 `packages: ["."]` 不含它。
- **版本线独立**：桌面 `v*` tag，移动 `mobile-v*` tag，两条流水线互不触发。

### 辅助目录

| 目录 | 用途 |
|---|---|
| `docs/` | Next.js 16 + Fumadocs 文档站，**并承载 Web 端 `/try`**（原 Astro + Starlight 已迁移） |
| `e2e/` | 桌面 e2e（WebdriverIO + tauri-plugin-wdio）与演示录制脚本 |
| `openspec/` | 变更提案（`changes/`）与规格（`specs/`）——需求与设计的落点 |
| `video/` | Remotion 后期工程（Demo / Hero 素材） |
| `scripts/` | `check-wasm.sh`、`check-zustand-store-access.mjs`、`web-bench` |
| `dev-notes/` | 知识库、博客、调研、归档；`archive/` 存重构前设计与已完成 roadmap |
| `dev-notes/prompts/` | **给新会话的启动提示词**（跨会话交接用）。与 `research/` 的区别：那边记「为什么这么决策」，这边记「接下来怎么开工」——含前置阅读、硬约束、已知坑、验收标准 |
| `dev-notes/research/` | **未落地方案**的调研与 spike 结论，每篇带决策状态。与 `knowledge/` 分开是刻意的——后者被 `/dev-workflow` 当「现行架构的事实」加载，混入调研会让人把「评估中的方案」读成「已有的能力」 |
| `dev-notes/incubation/` | **不属于本仓的独立项目构想**，每篇带孵化状态。与 `research/` 的区别：那边问「要不要在 SwarmDrop 里做」，这边问「要不要单独立项做」——`research/` 的每篇都隐含「落地点是本仓」，混进去会让人以为 SwarmDrop 要长出一个它不该有的子系统 |

### Auto-Update System

Updates are delivered by **[SwarmHive](https://github.com/swarm-apps/SwarmHive)** — a self-hosted,
open-source release & update server (same swarm-apps family). UpgradeLink has been **fully removed**.

- **Desktop** — Tauri updater 直连 SwarmHive，endpoint 在 `tauri.conf.json`。
  自托管服务器是明文 HTTP，故设了 `dangerousInsecureTransportProtocol: true`；Windows 用
  `installMode: "passive"`。
- **Mobile** — 同一台 SwarmHive，独立 app slug `swarmdrop-rn`（存量客户端指向它，**不能改名**）。
- 发布配置在 `swarmhive.toml`（server URL + app slug `swarmdrop`）。

### CI

| Workflow | 作用 |
|---|---|
| `rust.yml` | `cargo fmt --check` + `cargo check --workspace --all-targets` + `cargo test --workspace`；**wasm 双 target 门禁**（check + clippy）；clippy job 暂 `continue-on-error`（存量 warning 基线未清） |
| `release.yml` | `v*` tag 触发。generate-changelog → build-tauri（四目标 + 上传 SwarmHive draft）→ finalize-swarmhive → update-latest-json（仅手动 dispatch）→ publish-release |
| `mobile-release.yml` | `mobile-v*` tag，仅 Android |
| `mobile-build-android.yml` / `bootstrap-release.yml` / `docs.yml` | 移动构建 / 引导节点发布 / 文档站（含 develop → GitHub Pages） |

> Rust CI 目前**只跑 ubuntu**，Windows / macOS 的编译问题要到打 tag 才暴露。

## Important Conventions

- **Rust library naming:** lib 名为 `swarmdrop_lib`（非 `swarmdrop`），避开 Windows 上 cargo
  的 lib/bin 命名冲突。
- **libp2p git pin（本项目最大的单点依赖风险）。** `libp2p` / `libp2p-stream` /
  `libp2p-core` / `libp2p-swarm` / `libp2p-webrtc-utils` 同 pin
  `github.com/yexiyue/rust-libp2p` 一个 rev——**仍是个人 fork**。退出条件写死在
  `Cargo.toml` 对应注释里，且可判定。
  待合并的上游 PR：Web 端要的 #6558 / #6560。（#6570 relay 崩溃已自行关闭——
  上游 #6472 先修了同一问题。）**第三条是 #6576**：identify 的运行时 `agent_version`
  setter（2026-08-01 提）——没有它，改设备名必须重启整个节点。它独立于上面两个 PR，
  **不影响**「切官方 git」的判定，但**阻塞「删掉 fork pin」**。
  ⚠️ #6576 的 PR 分支（`feat/identify-set-agent-version`，rebase 到上游 master）与
  **本仓 pin 的分支**（`feat/identify-runtime-agent-version` @ `d858435c`）**不是同一条**，
  后者**绝不能 force-push**——commit 一游离就被 GC，构建当场断。
  退出：PR 均 MERGED → 切官方 git；crates.io 发布 0.57 → 切版本号依赖。
  详见 [`net-kernel.md`](dev-notes/knowledge/net-kernel.md) 的「临时 fork 集成策略」。
  **升级 rev 必须走独立 PR + 全量测试 + wasm check**，并同步 Cargo.lock。
- **webrtc-rs 现在零 pin，直接吃 crates.io `0.20.0`**（`rtc` 同版本）。五个功能补丁
  （rtc #137 / #138 / #140、webrtc #825 / #828）已随 **0.20.0 正式版**进 crates.io。
  2026-08-04 曾短暂 pin 过 fork 集成分支，为的是提前用上
  [webrtc#850](https://github.com/webrtc-rs/webrtc/pull/850) →
  [#853](https://github.com/webrtc-rs/webrtc/pull/853) 想公开的
  `gro_recv_buf_len` / `is_retryable_socket_recv_error`。**上游拒绝公开、PR 已关**：
  它们是 driver 的分配/分类策略，不属于 `AsyncUdpSocket` 契约，`pub` 等于把内部策略冻进
  1.0。理由成立，故两条 patch 整段删除、版本号退回 0.20.0。
  那两件事改由 `crates/webrtc-p2p/src/backend/native/direct/udp_mux.rs` **自己持有**
  （本 mux 把一个 UDP socket 多路复用给多个 PeerConnection，扮演的正是 driver 角色），
  依据是上游同时补进公开文档的两条规则：`AsyncUdpSocket::poll_recv` 的 `# Errors`
  （五个 transient 变体）与 `max_gro_segments` 的 `# Buffer sizing`（段长按路径 MTU
  1500，不按应用最大数据报）。**两者都没有反馈回路**（缓冲算小了内核静默丢尾部段、
  判据漏一种就把公网监听端口永久关掉），本仓最早那版两个都错了——那个文件里的两条护栏
  测试是唯一的兜底，**改实现必须同时改测试**。
  ⚠️ **别降回 `0.20.0-rc.*`**——rc 版不含五个补丁。
  ⚠️ **别降回 `0.20.0-rc.*`**——rc 版不含五个补丁，direct 监听端会起不来、数据面静默丢包。
  0.20.0 把 `AsyncUdpSocket` 换成 quinn 式 poll API，适配时连带修掉 `udp_mux` 三个既有
  缺陷（GRO 合并包没拆、判据漏 `ConnectionRefused` 使 Linux 监听端口可被远程掀掉、
  读循环无 burst 上限会饿死 swarm）。**前两个影响已发布的 v0.10.4 Linux 构建**，
  细节见 [`net-kernel.md`](dev-notes/knowledge/net-kernel.md)。
  五个补丁的完整复盘见 [`dev-notes/blogs/webrtc/`](dev-notes/blogs/webrtc/README.md)。
- **官方 `libp2p-webrtc` 与 `libp2p-webrtc-websys` 已于 2026-07-28 移除。** webrtc-direct
  改由自研的 `crates/webrtc-p2p` 提供，native 依赖树里的 webrtc-rs 从两套（0.17 + 0.20）
  并成一套。浏览器侧手动验证用 `cargo run -p webrtc-p2p --example direct_listener`。
  跨实现的证书兼容由 `certificate.rs` 的 `reads_official_pem_with_identical_certhash`
  钉死——**那条测试红了就说明存量地址会全部拨不通**。
- **Dev profile optimization:** 所有依赖在 dev 下也用 `opt-level = 3`（加密依赖否则慢 10–100 倍）。
  移动端 release 用单独的 `[profile.mobile-release]`，**它也是 `opt-level = 3`，且没有任何
  单包例外**（2026-08-10 从「包体优先 `opt-level = "z"` + blake3 单包覆写」改过来）。
  改的理由是 `"z"` 在传输热路径上要付数量级的代价，而热点遍布整棵依赖树、逐包例外列不全：
  blake3 的 `"z"` 会穿透进 build.rs 的 cc 调用按住 aarch64 NEON；WebRTC 的 DTLS 走 RustCrypto
  的**纯 Rust** AES-GCM/GHASH（不是 ring 的 asm），被 `-Oz` 关掉内联后，**同一台 Android 手机
  走 QUIC 有 12–23 MB/s、走 WebRTC 只剩 0.36–0.96 MB/s**。体积改由 `lto` + `codegen-units = 1`
  + `strip` 承担（三者都不牺牲速度）。完整推导与**尚未分离的那个变量**见
  [`toolchain.md`](dev-notes/knowledge/toolchain.md) 与
  [`2026-08-10-v0.15.2-field-test.md`](dev-notes/research/2026-08-10-v0.15.2-field-test.md)。
  注意 **Cargo 会静默忽略 member 自己的 profile**，只有 workspace root 的算数——
  单包覆写（`profile.*.package.*`）同样只认 root。
- **Vite port:** 固定 1420（Tauri 要求），HMR 1421。
- **Path alias:** `@/` → `./src/`（tsconfig 与 vite 一致）。
- **shadcn/ui:** `components.json` 用 `new-york` style、`rsc: false`、`neutral` base color、
  Lucide 图标，另注册了 `@aceternity` registry。
- **Diagrams:** 一律用 Mermaid，不用 ASCII art。
- **App identifier:** `com.yexiyue.swarmdrop`
- **Version management:** 两条版本线，各自有真源 + 跟随项，**同一条线内必须一起改**：
  - 桌面（tag `v*`）：真源 `src-tauri/tauri.conf.json`，跟随 `package.json` + `src-tauri/Cargo.toml`
  - 移动（tag `mobile-v*`）：真源 `mobile/app.json` 的 `expo.version`，跟随 `mobile/package.json`
    （**别漏这个**——CI 的 `verify-versions` 会拦，但那是打完 tag 才发现）；发版还要递增
    `expo.android.versionCode`

  完整表格与「为什么不统一版本线」见 [`toolchain.md`](dev-notes/knowledge/toolchain.md) 的
  「版本号同步」。

## Key File Locations

| Purpose | Path |
|---------|------|
| Tauri 命令薄壳 | `src-tauri/src/commands/` |
| Tauri Builder 装配 / 命令注册 | `src-tauri/src/setup.rs` |
| 自动生成的 IPC bindings | `src/lib/bindings.ts`（**勿手改**） |
| 网络内核 | `crates/net/`、`crates/net-base/` |
| 传输域 | `crates/transfer/` |
| 宿主端口层 | `crates/host/` |
| 配对邀请（PairInvite） | `crates/invite/` |
| SQL 存储实现（native-only） | `crates/storage-sql/` |
| Web 壳（wasm） | `crates/web/`（`store.rs` 是 IndexedDB 写穿的 `WebTransferStore`，`inbox.rs` 是它的收件箱表，`idb.rs` 是两者的底层），入口 `docs/app/app` |
| Zustand stores | `src/stores/` |
| Web 应用前端 | `docs/app/app/`（Next 应用区，非 fumadocs 文档） |
| 三端共享的纯视图逻辑 | `packages/shared-view/`（**归属判据见该包 README**；跨 workspace 接线的坑见 `dev-notes/knowledge/toolchain.md`） |
| 桌面 + Web 共享的文件浏览器 | `packages/file-browser/`（React DOM 组件 + 缩略图管线。**依赖协议必须 `file:` 不能 `link:`**，且改完要在 `docs/` 重装——理由见该包 README） |
| wasm 产物（入库） | `packages/swarmdrop-web/` |
| Web 应用区导航定义 | `docs/app/app/_lib/nav.ts`（路由/标题/图标/徽标单一事实源）+ `_components/app-nav.tsx` |
| 路由页面 | `src/routes/` |
| shadcn/ui 组件 | `src/components/ui/` |
| 前端翻译 catalog | `src/locales/{locale}/messages.po` |
| 后端翻译 catalog | `src-tauri/locales/*.toml` |
| Tauri capabilities | `src-tauri/capabilities/default.json` |
| 网络内核知识库 | `dev-notes/knowledge/net-kernel.md` |
| 重构决策与五道门经验 | `dev-notes/why-libp2p-not-iroh.md`、`dev-notes/knowledge/libp2p-wasm.md` |
| 重构系列博客 | `dev-notes/blogs/2026-07-net-refactor-series.md` |
| 配对重构系列博客 | `dev-notes/blogs/pairing-invite/README.md` |
| WebRTC 系列博客（零基础入门 + 上游补丁复盘） | `dev-notes/blogs/webrtc/README.md` |
| 传输吞吐系列博客（2026-08-10 真机实测复盘） | `dev-notes/blogs/transfer-throughput/README.md`（探针 vs 排除法、bao 的 O(n²)、两套加密栈、停等流控、`try_join` 的取消语义） |
| 产品需求 | `dev-notes/product-requirements.md` |
| UI 设计文件 | `dev-notes/design/design.pen` |
| 历史文档（重构前设计 / 已完成 roadmap / 早期调研） | `dev-notes/archive/` |
| 移动端 | `mobile/` |
| 移动端 Rust 桥接 (uniffi) | `mobile/packages/swarmdrop-core/rust/mobile-core/` |
| Release CI | `.github/workflows/release.yml` |

## Development Phases

| Phase | Status | Description |
|-------|--------|-------------|
| Phase 1 — Networking | Done | Swarm、mDNS、DHT、Relay、DCUtR |
| Phase 2 — Pairing | Done | 已从 6 位分享码演进为 PairInvite 一次性签名邀请 |
| Phase 3 — File Transfer | Done | 加密传输、断点续传、SQLite 历史与收件箱、MCP server |
| Phase 4 — Mobile | Done | React Native + Expo + uniffi，独立版本线 `mobile-v*` |
| Phase 5 — Web (wasm) | In Progress | `crates/web` + `docs/app/app`；WebRTC / relay 链路仍在收敛 |

Detailed per-phase specs: `dev-notes/archive/completed-roadmap/phase-*.md`（历史存档，
描述的是重构前的架构，读时注意时效）。

## Documentation Site

Next.js 16 + Fumadocs in `docs/`。内容在 `docs/content/docs/`。它同时是 Web 端的宿主
（`docs/app/app`），所以改 `crates/web` 后需 `pnpm build:wasm` 重新生成 wasm 产物。
Tutorial 风格内容用 `/swarmbook-tutorial` skill。
