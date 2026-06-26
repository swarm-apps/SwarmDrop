## Context

SwarmDrop currently treats the frontend Stronghold vault as the source of truth for the libp2p keypair and paired device list. The main route requires `auth-store.isUnlocked`, and unlocking initializes Stronghold with a user password before `secret-store` can hydrate and register the keypair with the backend.

This made sense while the pairing system lived mostly in the frontend, but the backend now owns `DeviceManager`, transfer history, MCP tools, and network lifecycle. Requiring the frontend to unlock Stronghold before the backend can start the node creates an unnecessary startup gate and conflicts with the planned React Native migration, where the SwarmNote Mobile pattern is to expose a `KeychainProvider` through UniFFI and store the device identity in OS secure storage.

The long-lived secret that must remain stable is the Ed25519 device identity keypair. Paired device metadata is sensitive enough to avoid casual exposure, but it is not encryption material and is already needed by backend services.

## Goals / Non-Goals

**Goals:**

- Start the app without requiring the user to enter an application password by default.
- Persist the libp2p keypair in the OS keychain on desktop and keep the model compatible with RN mobile secure storage.
- Move paired device persistence to backend-owned storage so network startup and MCP can operate without frontend Stronghold hydration.
- Preserve existing users' PeerId and paired devices through migration where possible.
- Keep optional biometric/app lock as a UI privacy layer, not as the keypair storage mechanism.

**Non-Goals:**

- Full RN mobile implementation. This design only prepares the identity model to match the future mobile app.
- Reworking the transfer encryption protocol.
- Encrypting every local database row. Transfer history and paired device metadata remain local app data.
- Building a password manager-style vault. SwarmDrop should behave like a file-transfer utility.

## Decisions

### Decision 1: Introduce a backend KeychainProvider abstraction

Create a Rust abstraction equivalent to SwarmNote's `KeychainProvider` with a `get_or_create_keypair() -> Vec<u8>` contract. The desktop implementation uses the OS keychain via the `keyring` crate.

Alternatives considered:
- Keep Stronghold as the identity backend and auto-unlock it with a stored random password. This reduces code churn but keeps an unnecessary vault layer and still forces the RN app to model Stronghold behavior.
- Store keypair bytes in SQLite. This is simple but weaker than system-provided keychain storage.

Rationale: The OS keychain is the natural platform boundary for a device identity secret. It also mirrors SwarmNote desktop and mobile.

### Decision 2: Move keypair generation and registration fully to the backend

Replace the frontend `generate_keypair` + `register_keypair` flow with backend commands that return the current device identity after loading or creating the keypair. Network `start` should read the keypair from backend state instead of receiving it indirectly through frontend Stronghold hydration.

Alternatives considered:
- Keep frontend keypair bytes and only change the storage adapter. This keeps too much security-sensitive behavior in the webview and makes RN reuse harder.

Rationale: The backend already owns libp2p startup and can validate/hold the keypair safely for the process lifetime.

### Decision 3: Persist paired devices in backend storage

Use the existing SeaORM/SQLite stack, or a thin backend store if simpler, to persist `PairedDeviceInfo`. `DeviceManager::new()` should load paired devices from this store on startup. Pairing accept/remove commands update both runtime `DeviceManager` and persistent storage.

Alternatives considered:
- Keep paired devices in frontend Zustand/Stronghold and inject them into `start()`. This preserves current behavior but blocks passwordless startup.
- Store paired devices in Tauri plugin store. This is acceptable, but the project already has SQLite and transfer history persistence.

Rationale: Paired devices are part of backend network state. Keeping them backend-owned reduces data-source drift and helps MCP tools.

### Decision 4: Replace mandatory auth routes with onboarding and optional app lock

`isSetupComplete` becomes onboarding/security state, not a vault setup flag. Main app routes no longer require `isUnlocked` unless optional app lock is enabled. Existing biometric code can be repurposed to unlock the UI session, but it must not store or reveal the Stronghold password.

Alternatives considered:
- Keep password setup but make biometric mandatory for convenience. This still leaves first-run users configuring a password for a tool that does not need one by default.

Rationale: The default product experience should be open-the-app-and-send, while privacy-conscious users can opt into app lock.

### Decision 5: Provide a one-time migration from Stronghold

For existing installs, if backend keychain has no keypair and the app detects a completed legacy setup, show a legacy migration unlock screen. After the user enters the old password or uses existing biometric unlock, load the old Stronghold data, save keypair bytes to the OS keychain, persist paired devices to backend storage, mark migration complete, and stop requiring Stronghold.

Alternatives considered:
- Generate a new keypair and ask users to re-pair devices. This is much simpler but breaks the current device identity and existing pairings.
- Keep legacy users on Stronghold forever. This creates two long-lived auth models.

Rationale: A one-time migration protects user trust without carrying the old default forever.

## Risks / Trade-offs

- [Keychain unavailable on Linux/headless environments] -> Return a clear degraded-state error or allow an explicitly marked ephemeral fallback. The UI must warn if PeerId will not persist.
- [Migration failure could lose identity] -> Migration must copy first, verify backend can load the copied keypair, then mark legacy migration complete. It must never delete the Stronghold vault automatically in the first implementation.
- [Paired device DB and runtime state diverge] -> Pairing/remove commands update persistence first, then runtime state, and refresh device snapshots after success.
- [Security perception changes] -> Settings should explain that the device identity is protected by the OS keychain and that app lock is optional screen privacy.
- [Desktop and future RN models drift] -> Use the same conceptual contracts: `KeychainProvider`, `EventBus`, backend-owned paired devices.

## Migration Plan

1. Add backend keychain storage and identity initialization behind new APIs while leaving legacy Stronghold flows untouched.
2. Add paired-device persistence and loading from backend storage.
3. Add migration detection and one-time migration from Stronghold into keychain/database.
4. Remove mandatory auth route gate and update onboarding/security routing.
5. Remove or deprecate frontend `secret-store` keypair ownership after migration paths are verified.

Rollback strategy: keep the legacy Stronghold vault and old password unlock code until one release after migration. If the new keychain path fails, users can retry migration or fall back to legacy unlock in that release.

## Open Questions

- Should Linux keychain failure block startup, or should SwarmDrop allow explicit ephemeral identity mode for portable/headless use?
- Should optional app lock support password-only, biometric-only, or both?
- Should paired devices be stored in the existing app SQLite database or a separate small backend store?
