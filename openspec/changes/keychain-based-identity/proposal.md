> **状态：已被 `extract-core-and-add-rn-mobile` 吸收（superseded）**
>
> 本变更的身份存储、免密码启动和已配对设备持久化方案已在 `extract-core-and-add-rn-mobile` 的任务 6.x 中作为 `KeychainProvider` host trait 整体实现，不再独立推进。后续如需调整身份存储策略，请直接在 core 端 / host adapter 中扩展。

## Why

SwarmDrop currently requires users to enter an application password on every launch before the device identity and paired devices can be loaded from Stronghold. This adds friction to a file-transfer tool whose long-lived secret is the device identity keypair, and SwarmNote already demonstrates a simpler cross-platform pattern using the OS keychain / secure store for that identity.

## What Changes

- **BREAKING**: Remove the default route gate that requires password unlock before entering the main app.
- Store the libp2p Ed25519 device identity keypair in the platform keychain instead of the frontend Stronghold-backed Zustand store.
- Persist paired devices in the backend database or an equivalent backend-owned store so the network layer can load them without frontend Stronghold hydration.
- Replace the current password setup flow with a lightweight onboarding/security model that can initialize identity automatically.
- Keep biometric/app lock as an optional UI privacy feature, separate from device identity storage.
- Provide a migration path from the existing Stronghold vault for users who already have a keypair and paired devices.
- Preserve current P2P behavior: stable PeerId across launches, existing pairings, automatic network start, and transfer history.

## Capabilities

### New Capabilities

- `keychain-identity`: Platform-backed device identity persistence, including keypair creation, loading, fallback behavior, and migration from Stronghold.
- `app-access-flow`: Startup, onboarding, optional app lock, and routing behavior after removing mandatory password unlock.
- `paired-device-persistence`: Backend-owned persistence for paired devices so device manager and network startup no longer depend on frontend Stronghold.

### Modified Capabilities

- None.

## Impact

- Frontend:
  - `src/stores/auth-store.ts`
  - `src/stores/secret-store.ts`
  - `src/lib/stronghold.ts`
  - `src/routes/_auth/*`
  - `src/routes/_auth.tsx`
  - `src/routes/_app.tsx`
  - `src/commands/identity.ts`
  - `src/commands/network.ts`
  - settings/security UI if present or added
- Backend:
  - `src-tauri/src/commands/identity.rs`
  - `src-tauri/src/commands/mod.rs`
  - `src-tauri/src/commands/pairing.rs`
  - `src-tauri/src/commands/mod.rs::start`
  - `src-tauri/src/device/*`
  - `src-tauri/src/database/*`
  - `src-tauri/src/lib.rs`
  - `src-tauri/Cargo.toml`
- Dependencies:
  - Add a desktop system keychain dependency, likely `keyring`.
  - Keep Stronghold temporarily for migration unless migration is deliberately manual.
  - Mobile RN follow-up should use `expo-secure-store` through the planned UniFFI core wrapper.
