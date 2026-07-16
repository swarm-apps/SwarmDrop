## Why

SwarmDrop desktop replaced the old transfer runtime with a projection-first lifecycle, epoch-based resume coordination, and a data-channel data plane. SwarmDrop-RN is still pinned to an older core tag and cannot safely build Inbox, Activity, or trust-policy UI until the mobile native bridge speaks the same runtime language.

## What Changes

- **BREAKING**: Move `packages/swarmdrop-core/rust/mobile-core` from the old `v0.5.3` transfer API to the latest shared SwarmDrop core runtime.
- Replace old history/status bridge types with transfer projection, phase/reason, epoch, recoverable, policy context, and projection file mirrors.
- Expose projection list/load APIs and projection update events to RN.
- Align pause, cancel, resume, accept, reject, completion, and failure commands/events with the shared core coordinator model.
- Rebuild generated TypeScript, C++ bridge files, and Android/iOS native artifacts.
- Validate Android artifact loading; iOS runtime validation is not required for this change.

## Capabilities

### New Capabilities

- `mobile-core-runtime-sync`: Mobile native bridge compatibility with the latest shared SwarmDrop runtime and data-plane APIs.
- `mobile-transfer-projection-api`: RN-accessible transfer projection records, commands, events, and status semantics.
- `mobile-native-artifact-generation`: Generated bindings and native artifact rebuild process for the breaking bridge update.

### Modified Capabilities

None. Existing archived specs are absent; this change supersedes the active `add-transfer-history` bridge model.

## Impact

- `packages/swarmdrop-core/rust/mobile-core`: dependencies, mirror records/enums, transfer/network event bridge, command implementations.
- `packages/swarmdrop-core/src/generated` and `packages/swarmdrop-core/cpp/generated`: regenerated bindings.
- Android native build outputs and iOS xcframework outputs.
- `src/core/event-bus.ts`, `src/stores/transfer-store.ts`, and transfer type helpers will be unblocked by the new bridge.
- Verification: Rust/mobile-core build, generated binding checksum checks, `pnpm typecheck`, Android native load/build smoke.
