# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 开发工作流

**IMPORTANT**：执行任何开发任务（编写代码、修改配置、添加依赖）前，必须先调用 `/dev-workflow` skill。它会加载项目知识库（`dev-notes/knowledge/`）中的最佳实践和踩坑记录，并在开发完成后引导更新知识库。

知识库主题：

- [`dev-notes/knowledge/theme-and-styling.md`](dev-notes/knowledge/theme-and-styling.md) — shadcn/ui、Tailwind、macOS Overlay 标题栏、Zustand selector 派生数组陷阱、Lingui 源 locale
- [`dev-notes/knowledge/rust-backend.md`](dev-notes/knowledge/rust-backend.md) — crates/core ↔ src-tauri 边界、specta + chrono、`#[expect]` 风格、IPC 时间类型选型
- [`dev-notes/knowledge/toolchain.md`](dev-notes/knowledge/toolchain.md) — Cargo dev profile opt-level、Vite/Tauri 端口、submodule、Lingui 实际 locale、版本号三处同步
- [`dev-notes/knowledge/iroh-migration.md`](dev-notes/knowledge/iroh-migration.md) — libp2p → iroh 迁移评估结论（2026-07 调研）：能力差、被推翻的旧认知、选型否决清单、前置实测清单
- [`dev-notes/knowledge/libp2p-wasm.md`](dev-notes/knowledge/libp2p-wasm.md) — Web 端（wasm）可行性（2026-07 调研）：局域网路线（最可行起点）、webrtc-direct 与自托管、浏览器公网零可达入口、rust-wasm 与 js-libp2p 取舍、tokio → n0-future、wasm 编译的坑
- [`dev-notes/knowledge/storage-abstraction.md`](dev-notes/knowledge/storage-abstraction.md) — 把 sea-orm 从 core 摘出去：切割线在 `DatabaseConnection` 不在 `entity`、SendWrapper 免改 trait、耦合面量化（**第 0 步已落地**：entity 的 sea-orm 已 feature 解绑）
- [`dev-notes/knowledge/net-kernel.md`](dev-notes/knowledge/net-kernel.md) — 网络内核 swarmdrop-net（2026-07 重构产物）：架构速览与事件双轨制、libp2p git master（pin 93c5059）校准坑 6 条（relay HOP status / NoAddressesInReservation / add_peer_address 非地址簿等）、wasm 工程约定、wire v2 契约点、已知负债

## Design Context

做任何 UI/视觉相关工作前，先读根目录的 `PRODUCT.md`（产品定位、目标用户、反面参考、设计原则）和 `DESIGN.md`（配色/字体/组件/elevation 等视觉令牌，含 `.impeccable/design.json` sidecar）。两者由 `/impeccable` 系列命令维护。

## Language

Always respond in Chinese (简体中文). All output, including thinking, planning, commit messages, and comments, must be in Chinese.

## Project Overview

SwarmDrop is a decentralized, cross-network, end-to-end encrypted file transfer tool built with Tauri v2. It aims to be a "cross-network version of LocalSend" — no accounts, no servers, supporting both LAN and cross-network peer-to-peer file transfers.

**Current Status:** Phase 3 (File Transfer) — transfer + resume + SQLite inbox in place; a local MCP server has shipped (AI agents can send files and search the received inbox). Current release: v0.7.7.

## Build and Development Commands

```bash
# Full app development (Vite frontend + Tauri Rust backend)
pnpm tauri dev

# Frontend only (Vite dev server at http://localhost:1420)
pnpm dev

# Production build
pnpm build              # Frontend (tsc + vite build)
pnpm tauri build        # Full app

# Rust (run from src-tauri/)
cargo build
cargo test
cargo test test_name    # Run a single test
cargo clippy
cargo fmt

# i18n — extract translation strings to .po files
pnpm i18n:extract

# Documentation site (run from docs/)
pnpm dev                # Astro + Starlight dev server
pnpm build

# Mobile (run from mobile/ — 独立 pnpm workspace)
pnpm install            # 必须在 mobile/ 下跑，根 workspace 不含它
pnpm ios                # expo run:ios
pnpm android            # expo run:android
pnpm typecheck
pnpm --filter react-native-swarmdrop-core build:ios      # 重建 uniffi 桥接
```

**Package manager:** pnpm only (not npm or yarn).

**Git submodule:** After cloning, run `git submodule update --init --recursive` to fetch the P2P library in `libs/`.

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Frontend | React 19, TypeScript 5.8, Vite 7, Tailwind CSS 4 |
| Routing | TanStack Router (file-system based, auto code-splitting) |
| State | Zustand 5 (stores: auth, network, preferences, secret, pairing, transfer) |
| UI | shadcn/ui (new-york style), Lucide icons, Radix primitives |
| i18n | Lingui 5 (3 locales: zh, zh-TW, en — ja/ko/es/fr/de are roadmap targets, not yet added) |
| Backend | Rust 2024, Tauri 2 |
| P2P | libp2p 0.56 via `swarm-p2p-core` (git submodule in `libs/`) |
| Security | Stronghold (encrypted vault), Biometry (FaceID/TouchID/Windows Hello) |
| Database | SeaORM 2.0 + SQLite (传输历史、断点续传 checkpoint) |

## Architecture

### Frontend → Backend Communication

Frontend calls Rust via Tauri IPC. TypeScript wrappers live in `src/commands/`:

```typescript
// src/commands/network.ts wraps invoke("start", ...)
import { invoke } from "@tauri-apps/api/core";
```

Rust command handlers are in `src-tauri/src/commands/` and registered in `src-tauri/src/lib.rs`:
```rust
.invoke_handler(tauri::generate_handler![commands::start, commands::shutdown, ...])
```

When adding a new Tauri command, use the `/edgemind-tauri-command` skill.

### Frontend Architecture

**Routing** — TanStack Router with file-system convention in `src/routes/`:
- `__root.tsx` — root layout
- `_auth.tsx` — unauthenticated layout (Aurora background). Guards redirect to `/devices` if already unlocked.
- `_auth/welcome.lazy.tsx`, `setup-password.lazy.tsx`, `unlock.lazy.tsx`, `enable-biometric.lazy.tsx`
- `_app.tsx` — authenticated layout (sidebar/bottom-nav). Guards redirect to `/welcome` or `/unlock` if not ready.
- `_app/devices.lazy.tsx`, `settings.lazy.tsx`, `pairing.tsx` + `pairing/*.lazy.tsx`
- `index.tsx` — redirects to `/devices`

Route guards use `beforeLoad` + `useAuthStore.getState()` to check auth state synchronously.

**State Management** — Zustand stores with different persistence backends:
- `auth-store` — auth flow state. Persisted to `localStorage` (only `isSetupComplete` + `biometricEnabled`).
- `preferences-store` — theme, language, device name. Persisted to `tauri-plugin-store`. Uses `onRehydrateStorage` to apply theme/language immediately, preventing flash.
- `secret-store` — Ed25519 keypair. Persisted to Stronghold encrypted vault via `src/lib/stronghold.ts`.
- `network-store` — runtime-only. Manages P2P node status, peer map (`Map<PeerId, PeerInfo>`), listen addresses, NAT status. Handles `NodeEvent` from Rust via Tauri Channel.
- `pairing-store` — runtime-only. Manages pairing code and pairing flow state.
- `transfer-store` — runtime-only. Manages active transfer sessions, pending offers queue, and in-memory history. Listens to `transfer-offer`/`transfer-progress`/`transfer-complete`/`transfer-failed` events.
- `update-store` / `upgrade-link-store` — app update checking via Tauri updater and UpgradeLink.

**Responsive Design** — 3 breakpoints via `use-breakpoint` hook:
- mobile (<768px): bottom navigation
- tablet (768–1023px): icon-only sidebar
- desktop (≥1024px): expanded sidebar

**i18n** — Lingui with Babel macro. Source locale is `zh`. Extract with `pnpm i18n:extract`. Catalogs in `src/locales/{locale}/messages.po`. Dynamic loading via `dynamicActivate(locale)`.

### Backend Architecture

`src-tauri` 现在是纯桌面壳。共享业务逻辑（network / pairing / device / protocol / database ops）
全在 `crates/core`（`swarmdrop-core`），`src-tauri/src/lib.rs` 通过 `pub use swarmdrop_core::pairing;`
等 alias 把命名空间桥接进来，所以代码里 `crate::pairing::*` / `crate::protocol::*` 路径仍然有效。

```
src-tauri/src/
├── lib.rs              # Tauri setup, plugin & command 注册, swarmdrop_core 模块 alias
├── main.rs             # Binary entry point
├── commands/           # Tauri IPC 命令薄壳，业务逻辑委托 swarmdrop-core / 本地 transfer 模块
│   ├── mod.rs          #   start / shutdown / list_devices / install_update
│   ├── identity.rs     #   initialize_identity / generate_keypair / register_keypair
│   ├── pairing.rs      #   generate_pairing_code / request_pairing / respond_pairing_request / remove_paired_device
│   ├── transfer.rs     #   scan_sources / prepare_send / start_send / accept_receive / 历史查询 / pause / resume
│   └── mcp.rs          #   桌面专用 MCP server 控制命令
├── network/
│   ├── mod.rs          #   NetManager 类型别名 + TransferRuntime impl
│   └── event_loop.rs   #   Tauri Channel 事件转发 + 持久化副作用
├── transfer/           # 本 crate 唯一仍含业务逻辑的模块
│   ├── mod.rs
│   ├── offer.rs        #   TransferManager —— Offer / Send / Resume 总入口
│   ├── sender.rs       #   发送端 chunk 推送
│   ├── receiver.rs     #   接收端落盘 + 断点续传
│   ├── crypto.rs       #   XChaCha20-Poly1305 加密/解密
│   └── progress.rs     #   ProgressTracker 的 Tauri Emitter 扩展 trait
├── file_source/        # 桌面文件读取（仅 Path::path_ops）
├── file_sink/          # 桌面文件写入（仅 Path::path_ops）
├── host/               # Desktop adapter：keychain / notifier / paths / update_installer / event_bus
├── database/mod.rs     # SeaORM 连接初始化 + 启动清理；re-export swarmdrop_storage_sql::{ops, inbox}
├── mcp/                # 桌面专用 MCP server
├── events.rs           # Tauri 事件名常量
└── error.rs            # AppError (thiserror) + AppResult
```

**Plugin initialization order** (in `lib.rs`):
1. Plugins registered in `Builder::default()`: store, os, fs, biometry, notification, opener, dialog, http, process
2. In `setup()`: updater is initialized after Builder; database (SeaORM + SQLite) is initialized then injected as Tauri state.

**Network startup flow:**
1. `commands::start()` creates `NodeConfig` with mDNS, relay, DCUtR, autonat, bootstrap peers
2. Calls `swarm_p2p_core::start::<AppRequest, AppResponse>()` → returns `(NetClient, Receiver<NodeEvent>)`
3. Spawns tokio tasks for DHT bootstrap and event forwarding to frontend via Channel
4. Creates `NetManager` (wraps `NetClient` + `PairingManager`), stores in Tauri state

**Tracing/Logging:** Uses `tracing` + `tracing-subscriber` with `EnvFilter`. Default filter: `swarmdrop=debug,swarm_p2p_core=debug`. Override with `RUST_LOG` env var.

**Bootstrap node:** One self-hosted node at `47.115.172.218:4001` (TCP + QUIC).

**Share code system:** 6-digit numeric codes. DHT key = SHA256(code). Records contain OS info + timestamp. Default TTL 300s.

### P2P Library (libs/)

Git submodule containing `swarm-p2p-core` crate. Workspace at `libs/Cargo.toml`, core code at `libs/core/`. Uses **Rust 2024 edition** — same as the main workspace (both `[workspace.package] edition = "2024"`; all member crates inherit via `edition.workspace = true`).

Key exports: `NetClient`, `NodeConfig`, `NodeEvent`, `start()`, re-exported `libp2p`.

### Mobile (`mobile/`)

移动端 (iOS / Android，React Native + Expo + uniffi) 曾是独立的 `swarm-apps/SwarmDrop-RN`
仓，现已并入本仓 `mobile/`（该仓已归档只读）。

- **Rust 桥接** `mobile/packages/swarmdrop-core/rust/mobile-core` 是根 Cargo workspace 的
  member，以 path 依赖引用 `crates/core` + `libs/core`。改 core 立刻对移动端生效，
  `cargo check --workspace` 一并覆盖。
- **JS 侧**是独立的 pnpm workspace（同 `video/`、`docs/`、`e2e/desktop`）——在 `mobile/`
  下跑 `pnpm install` / `pnpm typecheck`，根 workspace 的 `packages: ["."]` 不含它。
- **版本线独立**：桌面 `v*` tag，移动 `mobile-v*` tag，两条流水线互不触发。
  详见 `dev-notes/knowledge/toolchain.md`。

### Auto-Update System

Updates are delivered by **[SwarmHive](https://github.com/swarm-apps/SwarmHive)** — a self-hosted, open-source release & update server (same swarm-apps family). UpgradeLink has been **fully removed** (see `release.yml` header note).

- **Desktop** — the Tauri updater polls SwarmHive directly. Endpoint in `tauri.conf.json`:
  `http://47.115.172.218:3030/api/v1/updates/tauri/swarmdrop?current_version={{current_version}}&target={{target}}&arch={{arch}}`.
  The dogfood server is plain HTTP, so `dangerousInsecureTransportProtocol: true` is set; Windows uses `installMode: "passive"`.
- **Mobile** — `mobile/` checks updates against the same SwarmHive server, under a separate
  app slug `swarmdrop-rn` (存量客户端指向它，不能改名), published by `mobile-release.yml`.
- Publishing config lives in `swarmhive.toml` (server URL + app slug `swarmdrop`).

### Release Process

Triggered by pushing a `v*` tag. GitHub Actions workflow (`.github/workflows/release.yml`), publishing via `swarm-apps/swarmhive-action@v2`:
1. **generate-changelog** — derives release notes (shared by the GitHub Release body and SwarmHive notes)
2. **build-tauri** — builds desktop apps (macOS aarch64/x86_64, Ubuntu, Windows) via `tauri-action` (creates a GitHub draft release) and uploads each target's bundle to SwarmHive as a **draft**
3. **finalize-swarmhive** — after all targets upload, finalizes the SwarmHive release once (idempotent) and points the `stable` channel at it
4. **update-latest-json** — only on manual `workflow_dispatch` with `min_version` (forced updates); skipped on plain tag pushes
5. **publish-release** — converts the GitHub draft release to published

## Important Conventions

- **Rust library naming:** The lib is named `swarmdrop_lib` (not `swarmdrop`) to avoid a Windows cargo naming conflict between lib and bin targets.
- **Dev profile optimization:** All dependencies compiled with `opt-level = 3` even in dev mode. Crypto dependencies (`tauri-plugin-stronghold`, etc.) are 10–100x slower without this.
- **Vite port:** Fixed at 1420 (Tauri requirement). HMR on 1421.
- **Path alias:** `@/` maps to `./src/` in both TypeScript (`tsconfig.json`) and Vite (`vite.config.ts`).
- **shadcn/ui config:** `components.json` uses `new-york` style, `rsc: false`, `neutral` base color, Lucide icons. Also registers `@aceternity` registry.
- **Diagrams:** Always use Mermaid in markdown. No ASCII art.
- **App identifier:** `com.yexiyue.swarmdrop`
- **Version management:** Frontend version in `package.json`, Rust version in `src-tauri/Cargo.toml`, app version in `src-tauri/tauri.conf.json`. The `tauri.conf.json` version is the release version used in CI.

## Key File Locations

| Purpose | Path |
|---------|------|
| Tauri commands | `src-tauri/src/commands/` |
| Network module | `src-tauri/src/network/` |
| Pairing module | `src-tauri/src/pairing/` |
| Device module | `src-tauri/src/device/` |
| Frontend command wrappers | `src/commands/` |
| Zustand stores | `src/stores/` |
| Route pages | `src/routes/` |
| shadcn/ui components | `src/components/ui/` |
| Layout components | `src/components/layout/` |
| Translation catalogs | `src/locales/{locale}/messages.po` |
| Lingui config | `lingui.config.ts` |
| Product requirements | `dev-notes/product-requirements.md` |
| UI design file | `dev-notes/design/design.pen` |
| 网络内核（新，2026-07 重构） | `crates/net/`、`crates/net-base/` |
| 传输域（独立 crate） | `crates/transfer/`、宿主端口层 `crates/host/` |
| SQL 存储实现（native-only） | `crates/storage-sql/`（SessionStore/InboxStore 的 SeaORM 后端，core 零 sea-orm） |
| 网络内核知识库 | `dev-notes/knowledge/net-kernel.md` |
| 重构决策与五道门经验 | `dev-notes/why-libp2p-not-iroh.md`、`dev-notes/knowledge/libp2p-wasm.md` |
| 重构系列博客 | `dev-notes/blogs/2026-07-net-refactor-series.md` |
| 历史文档（重构前设计/已完成 roadmap/早期调研） | `dev-notes/archive/` |
| 移动端 (RN + Expo) | `mobile/` |
| 移动端 Rust 桥接 (uniffi) | `mobile/packages/swarmdrop-core/rust/mobile-core/` |
| 移动端 release CI | `.github/workflows/mobile-release.yml` |
| Tauri capabilities | `src-tauri/capabilities/default.json` |
| Release CI workflow | `.github/workflows/release.yml` |

## Development Phases

| Phase | Status | Description |
|-------|--------|-------------|
| Phase 1 — Networking | Done | libp2p Swarm, mDNS, DHT, Relay, DCUtR |
| Phase 2 — Pairing | Done | Share codes, device identity, DHT Provider |
| Phase 3 — File Transfer | In Progress | Request-Response, E2E encryption, SQLite history, pause/resume |
| Phase 4 — Mobile | Done | React Native + Expo + uniffi，已并入本仓 `mobile/`（独立版本线 `mobile-v*`）|

Detailed per-phase specs: `dev-notes/archive/completed-roadmap/phase-*.md`

## Documentation Site

Astro + Starlight in `docs/`. Content in `docs/src/content/docs/`. Use the `/swarmbook-tutorial` skill for tutorial-style content.
