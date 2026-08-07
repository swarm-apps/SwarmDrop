<div align="center">

<img src="public/app-icon.png" width="140" alt="SwarmDrop logo">

# SwarmDrop

**The data channel between your devices — for humans and AI agents alike.**

Decentralized, cross-network, end-to-end encrypted file transfer.
No accounts. No servers. No cloud.

[![Release](https://img.shields.io/github/v/release/swarm-apps/SwarmDrop?style=flat-square)](https://github.com/swarm-apps/SwarmDrop/releases)
[![License](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](LICENSE)
[![Platforms](https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux%20%7C%20Android%20%7C%20iOS-lightgrey?style=flat-square)](#download)
[![Stars](https://img.shields.io/github/stars/swarm-apps/SwarmDrop?style=flat-square)](https://github.com/swarm-apps/SwarmDrop/stargazers)
[![Built with Tauri](https://img.shields.io/badge/built%20with-Tauri%20v2-FFC131?style=flat-square&logo=tauri)](https://tauri.app)
[![Powered by libp2p](https://img.shields.io/badge/powered%20by-libp2p-3c5dd6?style=flat-square)](https://libp2p.io)

**[Website](https://swarm-apps.github.io/SwarmDrop/)** ·
**[Features](#features)** ·
**[AI & MCP](#built-for-ai-agents-mcp)** ·
**[Download](#download)** ·
**[Mobile](mobile/)** ·
**[简体中文](README.zh-CN.md)**

</div>

---

## About

**SwarmDrop** takes the LocalSend experience and frees it from the local network: send files securely between **any** of your devices, across any network, with **only the sender and receiver able to decrypt** them. No account to create, no central server in the middle.

And because the AI era runs on agents that constantly produce files on one machine and need them on another, SwarmDrop ships a **built-in local MCP Server** — so AI agents can deliver files across your devices and search your inbox, turning device-to-device transfer into **programmable infrastructure** for humans and agents alike.

## Features

| | |
|---|---|
| **Cross-Network** | Works on LAN or across the public internet. mDNS + Kademlia DHT + Relay + DCUtR pick the best route automatically — same Wi-Fi, different networks, behind NAT. |
| **End-to-End Encrypted** | Every connection is Noise- or TLS 1.3-encrypted and mutually authenticated. Relays forward ciphertext they hold no key for. Not a privacy *policy* — a cryptographic *fact*. |
| **No Accounts, No Servers** | Pair with a one-time signed invite (link or QR), or let LAN auto-discovery find your devices. Decentralized Ed25519 device identity. Self-host the bootstrap node if you want. |
| **AI-Native** | A local MCP Server lets AI agents drive transfers and search your received files — the part no AirDrop or LocalSend can do. |
| **Resumable & Reliable** | Resumable transfers with per-chunk BLAKE3 verification (bao-tree) — every block is verified as it lands, not after the whole file does. Plus a local SQLite history and inbox. Survives drops, restarts, and flaky links. |

## Built for AI Agents (MCP)

Most local tools can only be *read* by an AI. SwarmDrop can be *driven* by one.

It runs an embedded [Model Context Protocol](https://modelcontextprotocol.io) server — bound strictly to `127.0.0.1`, opt-in, off by default — that any local MCP client (Claude Desktop, Cursor, Claude Code, VS Code …) can connect to. Through it, an agent can:

- **Check the network** — is the P2P node up, who's connected.
- **List devices** — your paired, online devices.
- **Deliver files** — send files to a device by natural-language request; the recipient still approves in-app.
- **Search the inbox** — find what you've received by keyword, then resolve the local file path.

Everything happens on-device and end-to-end encrypted — the agent's reasoning can live in the cloud, but your files and their contents never leave your machines.

> See the [MCP usage guide](src-tauri/docs/mcp-guide.md) to wire it into your agent.

## Download

**[Get SwarmDrop from the official website](https://swarm-apps.github.io/SwarmDrop/)** — desktop and mobile, every platform in one place.

| Platform | Format |
|---|---|
| **macOS** | `.dmg` (Apple Silicon · Intel) |
| **Windows** | `.msi` · `.exe` (x64) |
| **Linux** | `.deb` · `.rpm` · `.AppImage` (x64) |
| **Android** | `.apk` |
| **iOS** | build from source |

> Downloads and **automatic updates** — for both desktop *and* mobile — are served by [SwarmHive](https://github.com/swarm-apps/SwarmHive), our own open-source, self-hostable release server. No proprietary update SaaS in the loop.

> **Mobile** — SwarmDrop also runs on **Android & iOS** via `mobile/`, a React Native app that shares the very same Rust core (`crates/core`) and encrypted protocol as the desktop app.

## Getting Started

```
1. Launch the app → name this device → start the P2P node
2. Add a device → share a one-time invite  /  LAN auto-discovery
3. Pick a device → drag & drop files to send
```

**Pairing**

- **Invite** — for cross-network: one side generates a one-time invite; send it as a link or scan the QR code. It carries an Ed25519 signature and a TTL, and can only be used once.
- **LAN** — on the same Wi-Fi, devices discover each other automatically; click to pair.

**Transfer paths** *(auto-selected, best first)*

| Route | Latency | When |
|---|---|---|
| Direct LAN | ~2 ms | same network |
| NAT hole-punch (DCUtR) | 10–100 ms | different networks, punch succeeds |
| Relay fallback | 100–500 ms | when hole-punching fails |

## Comparison

| | **SwarmDrop** | LocalSend | Syncthing |
|---|:---:|:---:|:---:|
| LAN transfer | ✓ | ✓ | ✓ |
| Cross-network (no shared network) | ✓ | — | ✓ <sub>(setup)</sub> |
| End-to-end encrypted | ✓ | ✓ | ✓ |
| No account / no server | ✓ | ✓ | ✓ |
| One-shot delivery (not continuous sync) | ✓ | ✓ | — |
| **AI agent-drivable (MCP)** | ✓ | — | — |
| Open source | ✓ | ✓ | ✓ |

## How It Works

```mermaid
graph TB
    subgraph Shells["Shells — desktop · mobile · web"]
        A["React + Tauri · React Native + uniffi · wasm"]
    end
    subgraph Core["Shared core — Rust (crates/*)"]
        B["transfer: chunked encryption · integrity · progress · resume"]
        G["pairing: one-time signed invites"]
    end
    subgraph Net["Network kernel — swarmdrop-net"]
        D["mDNS · LAN discovery"]
        E["Kademlia DHT · presence records"]
        F["Relay + DCUtR · NAT traversal"]
        H["TCP · QUIC · WebSocket · WebRTC-Direct"]
    end
    Shells -- "typed IPC / uniffi / wasm-bindgen" --> Core
    Core -- "Endpoint API" --> Net
```

**Security model**

- **Device identity** — Ed25519 keypair; the private key lives in the OS keychain (Keychain / Credential Manager / Secret Service).
- **Pairing** — one-time signed invites (a single canonical link): Ed25519 signature + 128-bit capability + TTL, consumable exactly once. Link, QR code, clipboard and deep link all carry the very same string; the capability rides in the URL fragment so it never reaches a server.
- **In-transit encryption** — Noise (TCP / WebSocket / WebRTC) or TLS 1.3 (QUIC). Every connection performs its own handshake with fresh ephemeral keys, and both peers are cryptographically authenticated by their device identity.
- **Integrity** — the file's BLAKE3 hash is the bao-tree verification root; every chunk carries a proof and is verified on arrival.
- **Zero trust** — bootstrap and relay nodes never see plaintext. Peers complete their own end-to-end handshake *on top of* the relay's byte pipe, so a relay holds no key for what it forwards.
- **No telemetry** — no data collection, ever.

<details>
<summary><b>Privacy &amp; telemetry</b></summary>

<br>

SwarmDrop collects **nothing**. There is no analytics, no account, and no central server that handles your files. File contents are encrypted end-to-end and only ever exist in plaintext on the sending and receiving devices. The optional MCP server binds to `127.0.0.1` only and is off by default. The only infrastructure involved is bootstrap/relay nodes that help peers find each other and relay **ciphertext** when direct connection fails — you can self-host your own.

</details>

<details>
<summary><b>Tech stack</b></summary>

<br>

| Layer | Technology |
|---|---|
| Frontend | React 19 · TypeScript 5.8 · Vite 7 · Tailwind CSS 4 · shadcn/ui |
| State / Routing | Zustand 5 · TanStack Router |
| i18n | Lingui 5 (zh · zh-TW · en) + rust-i18n for native strings |
| Backend | Rust 2024 · Tauri 2 · SeaORM + SQLite |
| P2P | in-house network kernel `swarmdrop-net` — iroh-style `Endpoint` API over libp2p (mDNS · Kademlia · Relay · DCUtR · WebRTC-Direct), native + wasm |
| Security | OS keychain · Ed25519 · Noise / TLS 1.3 (transport) · BLAKE3 + bao-tree |
| AI | embedded MCP server (rmcp + axum, `127.0.0.1` only) |
| IPC types | tauri-specta (commands & events, fully typed) |

</details>

<details>
<summary><b>Repository layout</b></summary>

<br>

```
SwarmDrop/
├── src/              # desktop frontend (React + Vite)
├── src-tauri/        # desktop shell (thin IPC commands, host adapters, MCP server, tray)
├── crates/
│   ├── net-base/     # network type foundation (NodeId / Addr / ProtocolId)
│   ├── net/          # network kernel: Endpoint facade + background actor
│   ├── host/         # platform-neutral host ports (DTO / error / device types)
│   ├── invite/       # PairInvite encoding + one-time registry + QR
│   ├── transfer/     # transfer domain (dependency-inverted via port traits)
│   ├── core/         # business core: identity / network / pairing / presence / protocol
│   ├── storage-sql/  # SeaORM + SQLite implementation of the storage ports (native only)
│   ├── web/          # browser shell, compiled to wasm
│   ├── bootstrap/    # public bootstrap + relay node
│   ├── entity/       # SeaORM entities
│   └── migration/    # SeaORM migrations
├── mobile/           # iOS / Android (React Native + Expo + uniffi)
└── docs/             # documentation site (Next.js + Fumadocs) — also hosts the web build at /try
```

The `crates/*` stack is shared by all three shells: desktop (`src-tauri`), mobile
(`mobile/`, via uniffi-bindgen-react-native), and web (`crates/web`, via wasm).
`crates/core` carries no `sea-orm` and `crates/transfer` no network dependency — those
boundaries are what keep the browser target compiling.

</details>

## Building from Source

Requires **Node 18+**, **pnpm 9+**, and a recent stable **Rust** toolchain (1.85+).
There are **no git submodules** — a plain clone is enough.

```bash
git clone git@github.com:swarm-apps/SwarmDrop.git
cd SwarmDrop
pnpm install

pnpm tauri dev      # develop
pnpm tauri build    # package
```

## Roadmap

- [x] P2P networking (mDNS · DHT · Relay · DCUtR)
- [x] Device pairing (one-time signed invites · QR · LAN direct)
- [x] File transfer (E2E encryption · live progress · history · resume)
- [x] MCP server — AI agents can send files & search the inbox
- [x] Mobile apps (iOS / Android)
- [ ] Web client in the browser (wasm) — try it at [`/app`](https://swarm-apps.github.io/SwarmDrop/app)
- [ ] Expanded agent toolset — full transfer lifecycle (status / cancel / pause / resume) over MCP
- [ ] On-device content extraction for richer inbox search

## Contributing

Issues and PRs welcome. A few conventions:

- [Conventional Commits](https://www.conventionalcommits.org) (`feat:` / `fix:` / `chore:` …).
- Before committing: `cargo fmt && cargo clippy` for Rust, `pnpm exec tsc --noEmit` for the frontend.
- IPC bindings (`src/lib/bindings.ts`) are **auto-generated** — don't hand-edit; run `pnpm tauri dev` to regenerate.
- **Translations** are managed with [Lingui](https://lingui.dev) (`pnpm i18n:extract`). New-language README contributions are welcome too — see [README.zh-CN.md](README.zh-CN.md) for the format.

## The swarm-apps Family

SwarmDrop is part of a family of decentralized, local-first, end-to-end encrypted tools:

- **SwarmDrop** — device-to-device file transfer, desktop and mobile in one repo. [Repo](https://github.com/swarm-apps/SwarmDrop)
- **SwarmNote** — decentralized, encrypted notes. [Desktop](https://github.com/swarm-apps/SwarmNote) · [Mobile](https://github.com/swarm-apps/SwarmNote-RN)
- **SwarmHive** — self-hostable, open-source release & auto-update server for Tauri and React Native apps. SwarmDrop ships every update through it — and so can you. [Repo](https://github.com/swarm-apps/SwarmHive)

## License

[MIT](LICENSE) © SwarmDrop Contributors

<div align="center"><sub>Built with <a href="https://tauri.app">Tauri</a> · <a href="https://libp2p.io">libp2p</a></sub></div>
