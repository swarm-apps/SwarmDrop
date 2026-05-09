# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Language

Always respond in Chinese (简体中文). All output, including thinking, planning, commit messages, and comments, must be in Chinese.

## Project Overview

SwarmDrop is a decentralized, cross-network, end-to-end encrypted file transfer tool built with Tauri v2. It aims to be a "cross-network version of LocalSend" — no accounts, no servers, supporting both LAN and cross-network peer-to-peer file transfers.

**Current Status:** Phase 3 (File Transfer) — pairing complete, file transfer + database integration in progress (v0.3.2).

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

# Android
pnpm android:dev        # Dev mode (sets SODIUM_LIB_DIR via scripts/android.mjs)
pnpm android:build

# Documentation site (run from docs/)
pnpm dev                # Astro + Starlight dev server
pnpm build
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
| i18n | Lingui 5 (8 locales: zh, zh-TW, en, ja, ko, es, fr, de) |
| Backend | Rust 2021, Tauri 2 |
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

```
src-tauri/src/
├── lib.rs              # Tauri setup, plugin registration, command handler
├── main.rs             # Binary entry point
├── mobile.rs           # Mobile entry point (cfg(mobile))
├── commands/
│   ├── mod.rs          # start/shutdown, command exports
│   ├── identity.rs     # generate_keypair, register_keypair
│   └── pairing.rs      # generate_pairing_code, get_device_info, request/respond_pairing
├── network/
│   ├── mod.rs          # Module exports
│   ├── config.rs       # NodeConfig builder (mDNS, relay, DCUtR, autonat, bootstrap)
│   ├── manager.rs      # NetManager — wraps NetClient + PairingManager
│   └── event_loop.rs   # NodeEvent forwarding to frontend via Tauri Channel
├── pairing/
│   ├── mod.rs          # Module exports
│   ├── code.rs         # 6-digit share code generation
│   ├── dht_key.rs      # SHA256→DHT key derivation
│   └── manager.rs      # PairingManager — DHT publish/query, online/offline announce
├── device/
│   ├── mod.rs          # Module exports
│   ├── manager.rs      # Device list management
│   └── utils.rs        # OsInfo — hostname, platform, agent_version string
├── transfer/
│   ├── mod.rs          # 模块导出
│   ├── session.rs      # TransferSession 状态管理
│   ├── sender.rs       # 发送端逻辑
│   ├── receiver.rs     # 接收端逻辑（含断点续传）
│   ├── chunker.rs      # 文件分块读取（FileChunker）
│   ├── assembler.rs    # 文件重组写入（FileAssembler）
│   ├── crypto.rs       # XChaCha20-Poly1305 加密/解密
│   ├── progress.rs     # 进度追踪 + 速度计算
│   └── error.rs        # 传输错误类型
├── database/
│   ├── mod.rs          # SeaORM 连接初始化
│   └── entity/         # SeaORM 2.0 实体（transfer_sessions, transfer_files, file_checkpoints）
├── protocol.rs         # AppRequest/AppResponse — CBOR over libp2p Request-Response
└── error.rs            # AppError (thiserror), AppResult
```

**Plugin initialization order** (in `lib.rs`):
1. Plugins registered in `Builder::default()`: store, os, fs, biometry, notification, opener, process, mobile
2. In `setup()`: updater (with mobile fallback — silently skips if unsupported), then Stronghold (needs `salt_path` from app data dir)

**Network startup flow:**
1. `commands::start()` creates `NodeConfig` with mDNS, relay, DCUtR, autonat, bootstrap peers
2. Calls `swarm_p2p_core::start::<AppRequest, AppResponse>()` → returns `(NetClient, Receiver<NodeEvent>)`
3. Spawns tokio tasks for DHT bootstrap and event forwarding to frontend via Channel
4. Creates `NetManager` (wraps `NetClient` + `PairingManager`), stores in Tauri state

**Tracing/Logging:** Uses `tracing` + `tracing-subscriber` with `EnvFilter`. Default filter: `swarmdrop=debug,swarm_p2p_core=debug`. Override with `RUST_LOG` env var.

**Bootstrap node:** One self-hosted node at `47.115.172.218:4001` (TCP + QUIC).

**Share code system:** 6-digit numeric codes. DHT key = SHA256(code). Records contain OS info + timestamp. Default TTL 300s.

### P2P Library (libs/)

Git submodule containing `swarm-p2p-core` crate. Workspace at `libs/Cargo.toml`, core code at `libs/core/`. Uses **Rust 2024 edition** (different from the main app's 2021 edition).

Key exports: `NetClient`, `NodeConfig`, `NodeEvent`, `start()`, re-exported `libp2p`.

Android-specific: DNS feature is disabled (`/etc/resolv.conf` doesn't exist on Android). Configured via conditional dependencies in `src-tauri/Cargo.toml`:
```toml
[target.'cfg(not(target_os = "android"))'.dependencies]
swarm-p2p-core = { path = "../libs/core", features = ["dns"] }
```

### Auto-Update System

Dual-endpoint update checking:
1. **UpgradeLink** — `https://api.upgrade.toolsetlink.com/v1/tauri/upgrade?tauriKey=...` (primary)
2. **GitHub Releases** — `https://github.com/swarm-apps/SwarmDrop/releases/latest/download/latest.json` (fallback)

The `latest.json` is patched in CI to include Android APK info (`mobile.android` field) and optional `min_version` for forced updates. Windows uses passive install mode.

### Release Process

Triggered by pushing a `v*` tag. GitHub Actions workflow (`.github/workflows/release.yml`):
1. **build-tauri** — Builds desktop apps (macOS aarch64/x86_64, Ubuntu, Windows) via `tauri-action`, creates draft release
2. **build-android** — Builds Android APK (aarch64), uploads to the draft release
3. **update-latest-json** — Patches `latest.json` with Android download URL and optional `min_version`
4. **publish-release** — Converts draft to published release (required for UpgradeLink to access assets)
5. **upgradeLink-upload** — Syncs desktop and Android builds to UpgradeLink service

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
| Implementation roadmap | `dev-notes/roadmap/implementation-roadmap.md` |
| UI design file | `dev-notes/design/design.pen` |
| 传输场景设计 | `dev-notes/design/transfer-scenarios-design.md` |
| 数据库实体设计 | `dev-notes/design/database-entity-design.md` |
| 文件传输设计 | `dev-notes/design/file-transfer-design.md` |
| P2P core library | `libs/core/` |
| Tauri capabilities | `src-tauri/capabilities/default.json` |
| Release CI workflow | `.github/workflows/release.yml` |

## Development Phases

| Phase | Status | Description |
|-------|--------|-------------|
| Phase 1 — Networking | Done | libp2p Swarm, mDNS, DHT, Relay, DCUtR |
| Phase 2 — Pairing | Done | Share codes, device identity, DHT Provider |
| Phase 3 — File Transfer | In Progress | Request-Response, E2E encryption, SQLite history, pause/resume |
| Phase 4 — Mobile | Pending | HTTP bridge or libp2p full-platform, QR code pairing |

Detailed per-phase specs: `dev-notes/roadmap/phase-*.md`

## Documentation Site

Astro + Starlight in `docs/`. Content in `docs/src/content/docs/`. Use the `/swarmbook-tutorial` skill for tutorial-style content.
