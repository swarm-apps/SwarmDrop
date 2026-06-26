## Why

SwarmDrop's mobile direction should move from Tauri mobile to a dedicated React Native app, but the current Rust backend is still a Tauri host implementation rather than a reusable core. Extracting a host-agnostic `swarmdrop-core` first lets desktop and RN share the same P2P, pairing, transfer, identity, and persistence behavior instead of reimplementing the most important logic twice.

## What Changes

- **BREAKING**: Reorganize the Rust workspace so app logic moves from `src-tauri/src/*` into reusable crates under `crates/`, with `src-tauri` becoming the desktop host adapter.
- Create `swarmdrop-core` as the platform-independent Rust core for identity, device management, pairing, P2P lifecycle, transfer protocol, transfer persistence, and core events.
- Move SeaORM `entity` and `migration` crates into the shared workspace layout used by the core.
- Introduce host traits/adapters for keychain identity, event delivery, file source/sink access, app data paths, notifications, and platform update/install behavior.
- Absorb `keychain-based-identity` into the core extraction as the identity host boundary, rather than implementing it as a standalone pre-step.
- Refactor the Tauri desktop app to use `swarmdrop-core` through desktop host adapters while preserving current desktop features.
- Create a sibling React Native project, `../swarmdrop-mobile`, based on the SwarmNote Mobile pattern: Expo, Expo Router, NativeWind, Zustand, Lingui, RN primitives, and a UniFFI Turbo Module package.
- Create `packages/swarmdrop-core` inside the RN app to wrap `swarmdrop-core` with `uniffi-bindgen-react-native`.
- Deliver an RN mobile MVP that can initialize identity, run onboarding, pair devices, show device/network state, and perform foreground send/receive flows.
- Keep advanced mobile file access, background transfers, app-store release, and full desktop/RN UI parity out of the first milestone unless needed for MVP validation.

## Capabilities

### New Capabilities

- `swarmdrop-core`: Host-agnostic Rust core exposing the shared application model and runtime services.
- `desktop-host-adapter`: Tauri desktop host integration for IPC, events, filesystem access, keychain, notifications, updater, and current UI commands.
- `rn-mobile-project`: React Native mobile project scaffold and runtime architecture matching the SwarmNote Mobile approach.
- `uniffi-mobile-bridge`: UniFFI/Turbo Module bridge exposing selected `swarmdrop-core` APIs to React Native.
- `host-identity-storage`: Cross-host identity storage model, including keychain-backed device identity and explicit deprecation of legacy Stronghold data.
- `host-file-access`: Cross-host file source/sink abstractions for desktop paths, existing Android/Tauri file access, and RN mobile file picker/cache/public-dir behavior.
- `mobile-mvp-flow`: First RN mobile product flow for onboarding, device pairing, network status, and foreground file transfer.

### Modified Capabilities

- None.

## Impact

- Rust workspace:
  - Root `Cargo.toml`
  - New `crates/core`
  - Existing `src-tauri/entity` and `src-tauri/migration`
  - Existing `libs/core` path dependency
- Tauri desktop:
  - `src-tauri/src/lib.rs`
  - `src-tauri/src/commands/*`
  - `src-tauri/src/network/*`
  - `src-tauri/src/pairing/*`
  - `src-tauri/src/transfer/*`
  - `src-tauri/src/device/*`
  - `src-tauri/src/file_source/*`
  - `src-tauri/src/file_sink/*`
  - `src-tauri/src/database/*`
  - `src-tauri/src/mobile.rs`
  - `src-tauri/Cargo.toml`
- Desktop frontend:
  - `src/commands/*`
  - `src/stores/*`
  - auth/onboarding routes if identity storage changes
  - transfer, pairing, device, settings routes
- New RN project:
  - `../swarmdrop-mobile/package.json`
  - `../swarmdrop-mobile/app.json`
  - `../swarmdrop-mobile/src/app/*`
  - `../swarmdrop-mobile/src/core/*`
  - `../swarmdrop-mobile/src/stores/*`
  - `../swarmdrop-mobile/packages/swarmdrop-core/*`
- Dependencies:
  - `uniffi` and `uniffi-bindgen-react-native`
  - Expo/RN stack equivalent to SwarmNote Mobile
  - likely `keyring` on desktop and `expo-secure-store` on RN
- OpenSpec:
  - `keychain-based-identity` should be treated as absorbed or superseded by this broader change unless the user later chooses to split it back out.
