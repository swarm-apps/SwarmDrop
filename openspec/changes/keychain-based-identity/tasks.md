## 1. Backend Keychain Identity

- [ ] 1.1 Add a backend `KeychainProvider` abstraction for loading or creating libp2p protobuf-encoded Ed25519 keypair bytes
- [ ] 1.2 Add a desktop `KeyringKeychain` implementation backed by the OS keychain using the `keyring` crate
- [ ] 1.3 Add backend identity state that loads the keypair once during app setup and exposes PeerId/device identity
- [ ] 1.4 Refactor `commands::generate_keypair` and `commands::register_keypair` into compatibility or replacement APIs that no longer require frontend keypair ownership
- [ ] 1.5 Update the network `start` command to use backend-held keypair state instead of `State<Keypair>` registered by the frontend
- [ ] 1.6 Add unit tests for first-launch keypair generation and restart identity reuse using an in-memory keychain provider

## 2. Paired Device Persistence

- [ ] 2.1 Add a SeaORM migration/entity or backend store for persisted paired devices keyed by PeerId
- [ ] 2.2 Implement database operations to list, upsert, and delete paired devices
- [ ] 2.3 Load persisted paired devices into `DeviceManager` during network manager initialization
- [ ] 2.4 Update pairing success flow to persist `PairedDeviceInfo` before updating runtime `DeviceManager`
- [ ] 2.5 Update unpair flow to delete persistent paired device data before updating runtime `DeviceManager`
- [ ] 2.6 Update frontend `network.start()` wrapper so it no longer passes `pairedDevices`
- [ ] 2.7 Add backend tests for paired device persistence and duplicate upsert behavior

## 3. Legacy Stronghold Migration

- [ ] 3.1 Add migration state flags to record whether legacy identity migration is pending, complete, or not applicable
- [ ] 3.2 Implement legacy migration command that accepts the old password, opens Stronghold, reads keypair and paired devices, and copies them to keychain/database
- [ ] 3.3 Verify migrated keypair by reloading it from keychain and comparing the derived PeerId before marking migration complete
- [ ] 3.4 Ensure migration failure leaves the Stronghold vault and migration flags unchanged
- [ ] 3.5 Add frontend migration screen for existing users whose keychain identity is missing but legacy setup state exists
- [ ] 3.6 Reuse existing biometric retrieval only for legacy migration unlock, not for normal app startup
- [ ] 3.7 Add migration tests for success, wrong password, keychain write failure, and duplicate paired devices

## 4. Frontend Access Flow

- [ ] 4.1 Replace mandatory password setup routing with onboarding/security state that does not require password creation by default
- [ ] 4.2 Remove `isUnlocked` as a default requirement from `_app.tsx` route guards
- [ ] 4.3 Refactor or remove `secret-store` keypair ownership so keypair bytes are no longer persisted in frontend Zustand
- [ ] 4.4 Update app initialization to call backend identity initialization before auto-starting the network
- [ ] 4.5 Keep optional app lock state separate from identity storage and route gates
- [ ] 4.6 Update unlock/auth copy and i18n strings to explain migration or optional app lock instead of mandatory vault unlock

## 5. Settings and User Feedback

- [ ] 5.1 Add settings UI for enabling/disabling optional app lock if it remains in scope for this change
- [ ] 5.2 Show clear degraded-state feedback when persistent keychain access is unavailable
- [ ] 5.3 Show migration success/failure feedback with retry guidance
- [ ] 5.4 Ensure settings/about displays the current PeerId loaded from backend identity state

## 6. Verification

- [ ] 6.1 Run `pnpm build`
- [ ] 6.2 Run `cargo test` in `src-tauri`
- [ ] 6.3 Run `cargo clippy` in `src-tauri`
- [ ] 6.4 Manually verify fresh install: onboarding completes without password and app enters `/devices`
- [ ] 6.5 Manually verify existing install migration preserves PeerId and paired devices
- [ ] 6.6 Manually verify auto-start network works after app restart without password input
- [ ] 6.7 Manually verify optional app lock, if enabled, gates UI without affecting network identity loading
