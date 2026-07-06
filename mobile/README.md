<a name="readme-top"></a>

<div align="center">

<img src="assets/images/readme-icon.png" width="140" alt="SwarmDrop Mobile logo">

# SwarmDrop Mobile

### Cross-network, end-to-end encrypted file transfer — now in your pocket.

The Android & iOS client for [SwarmDrop](https://github.com/swarm-apps/SwarmDrop),
sharing the very same Rust core as the desktop app.

[![Release](https://img.shields.io/github/v/release/swarm-apps/SwarmDrop-RN?style=flat-square)](https://github.com/swarm-apps/SwarmDrop-RN/releases)
[![License](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](LICENSE)
[![Platforms](https://img.shields.io/badge/platform-Android%20%7C%20iOS-lightgrey?style=flat-square)](#-download)

[![Expo](https://img.shields.io/badge/Expo-SDK%2056-000020?style=flat-square&logo=expo)](https://expo.dev)
[![React Native](https://img.shields.io/badge/React%20Native-0.85-61DAFB?style=flat-square&logo=react)](https://reactnative.dev)

**English** · [简体中文](README.zh-CN.md)

[Website](https://swarm-apps.github.io/SwarmDrop/) · [Features](#-features) · [Download](#-download) · [Develop](#-development) · [Desktop](https://github.com/swarm-apps/SwarmDrop)

</div>

<!-- TODO(assets): add a short demo GIF here — pair with a desktop, receive a file on the phone, ~12s.
     Recommended path: docs/screenshots/mobile-demo.gif -->

---

## About

SwarmDrop Mobile brings SwarmDrop's **cross-network P2P file transfer** to Android and iOS: send and receive files between any of your devices, across any network, with **only the sender and receiver able to decrypt** them — no accounts, no servers.

It shares the **same Rust core** (`swarmdrop-core`) as the desktop app, bridged into a React Native Turbo Module via [uniffi-bindgen-react-native][ubrn]. Same encrypted protocol, same pairing — it just talks to your laptop.

> This is the sibling repo of [**SwarmDrop**](https://github.com/swarm-apps/SwarmDrop). For the product vision, security model, and protocol details, see the main repo's README.

## ✨ Features

| | | |
|---|---|---|
| 🔗 **Cross-platform pairing** | Pairs and transfers with the desktop app over the exact same P2P protocol. | <sub>phone ↔ laptop, across networks</sub> |
| 🔒 **End-to-end encrypted** | XChaCha20-Poly1305 with a fresh per-transfer key; relays never see plaintext. | <sub>same crypto as desktop</sub> |
| 📁 **Save anywhere (SAF)** | Write to Downloads or any user-picked folder, with resumable, safe chunked writes. | <sub>Android Storage Access Framework</sub> |
| ⚡ **Auto-receive** | Files from already-paired devices land in the background — no manual tap. | <sub>matches the passive-receive model</sub> |

<div align="right"><a href="#readme-top">↑ back to top</a></div>

## 📦 Download

**[Get SwarmDrop from the official website →](https://swarm-apps.github.io/SwarmDrop/)** — every platform, desktop and mobile, in one place.

| Platform | Format |
|---|---|
| **Android** | `.apk` — [SwarmDrop-RN releases](https://github.com/swarm-apps/SwarmDrop-RN/releases/latest) (arm64-v8a) |
| **iOS** | build from source (see [Development](#-development)); not yet on TestFlight |

> 🔄 In-app **automatic updates** are delivered by [**SwarmHive**](https://github.com/swarm-apps/SwarmHive) — the same self-hostable, open-source update server the desktop app uses. No proprietary update SaaS.

<div align="right"><a href="#readme-top">↑ back to top</a></div>

## 🛠 Development

Requires **Node 22+** · **pnpm 10+** · a recent stable **Rust** (1.85+) · **JDK 17** · **Android NDK r27c** (plus **Xcode 16+** for iOS).

```bash
git clone git@github.com:swarm-apps/SwarmDrop-RN.git
cd SwarmDrop-RN
pnpm install

# Build the Rust bridge (pick your platform)
pnpm --filter react-native-swarmdrop-core build:android
pnpm --filter react-native-swarmdrop-core build:ios

# Generate the native projects
pnpm prebuild

# Run it
pnpm android        # or: pnpm ios
```

> Expo Go won't work — the app loads native Rust bindings, so you need a dev client.

<details>
<summary><b>Windows long-path issue</b></summary>

<br>

On Windows, ubrn's target directory can exceed the path-length limit and hit a linker error. Point `CARGO_TARGET_DIR` at a short path:

```powershell
$env:CARGO_TARGET_DIR='D:\tmp\swarmdrop-mobile-core-target'
pnpm --filter react-native-swarmdrop-core build:android
```

</details>

<details>
<summary><b>Linking the main repo locally</b></summary>

<br>

[`packages/swarmdrop-core/rust/mobile-core/Cargo.toml`][mobile-cargo] pulls the core over HTTPS from the main repo by default (used in CI). To hack on core locally, switch to path deps:

```toml
swarmdrop-core = { path = "../../../../../SwarmDrop/crates/core" }
entity         = { path = "../../../../../SwarmDrop/crates/entity" }
migration      = { path = "../../../../../SwarmDrop/crates/migration" }
swarm-p2p-core = { path = "../../../../../SwarmDrop/libs/core" }
```

Don't mix git and path deps (`swarm-p2p-core` will hit a multiple-versions conflict).

</details>

<details>
<summary><b>Tech stack</b></summary>

<br>

| Layer | Technology |
|---|---|
| Framework | Expo SDK 56 · React Native 0.85 · React 19 · expo-router |
| UI | NativeWind v5 (Tailwind) · rn-primitives · lucide-react-native |
| State | Zustand 5 · AsyncStorage persistence |
| i18n | Lingui 5 (zh · en) |
| Native bridge | [uniffi-bindgen-react-native][ubrn] (Turbo Module) |
| Rust core | `swarmdrop-core` / `swarm-p2p-core` (shared with desktop) |
| File I/O | expo-file-system 56 (SAF chunked writes) |
| Updates | SwarmHive engine (registry-rn) |

</details>

<details>
<summary><b>Repository layout</b></summary>

<br>

```
SwarmDrop-RN/
├── src/                              # RN app code
│   ├── app/                          #   expo-router routes
│   ├── components/                   #   UI components
│   ├── core/                         #   ForeignFileAccess / event-bus / paths
│   └── stores/                       #   zustand stores
├── packages/swarmdrop-core/          # ubrn package
│   ├── rust/mobile-core/             #   the bridge Rust crate
│   └── src/                          #   generated TS bindings
├── plugins/                          # Expo config plugins (release signing, install permission)
└── dev-notes/                        # project knowledge base
```

</details>

<div align="right"><a href="#readme-top">↑ back to top</a></div>

## 🚢 CI / Release

- **Build Android** (manual): [`workflows/build-android.yml`](.github/workflows/build-android.yml)
- **Release** (on tag push): [`workflows/release.yml`](.github/workflows/release.yml)
  - git-cliff changelog → ubrn release build → signed APK → GitHub draft release → publish → **SwarmHive** (upload + finalize → `stable` channel)

Cut a release:

```bash
# bump version in package.json + app.json
git commit -am "chore(release): vX.Y.Z"
git tag vX.Y.Z && git push origin main && git push origin vX.Y.Z
```

## ❤️ The swarm-apps family

Part of a family of decentralized, local-first, end-to-end encrypted tools:

- **SwarmDrop** — device-to-device file transfer. [Desktop](https://github.com/swarm-apps/SwarmDrop) · [Mobile](https://github.com/swarm-apps/SwarmDrop-RN)
- **SwarmNote** — decentralized, encrypted notes. [Desktop](https://github.com/swarm-apps/SwarmNote) · [Mobile](https://github.com/swarm-apps/SwarmNote-RN)
- **SwarmHive** — self-hostable, open-source release & auto-update server for Tauri and React Native apps. [Repo](https://github.com/swarm-apps/SwarmHive)

## 📄 License

[MIT](LICENSE) © SwarmDrop Contributors

<div align="center"><sub>Built with <a href="https://expo.dev">Expo</a> · <a href="https://github.com/jhugman/uniffi-bindgen-react-native">uniffi-bindgen-react-native</a></sub></div>

<div align="right"><a href="#readme-top">↑ back to top</a></div>

[ubrn]: https://github.com/jhugman/uniffi-bindgen-react-native
[mobile-cargo]: packages/swarmdrop-core/rust/mobile-core/Cargo.toml
