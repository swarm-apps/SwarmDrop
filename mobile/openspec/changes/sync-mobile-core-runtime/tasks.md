## 1. Dependency Sync

- [ ] 1.1 Select and record the target SwarmDrop shared core commit or tag.
- [ ] 1.2 Update `packages/swarmdrop-core/rust/mobile-core/Cargo.toml` dependencies.
- [ ] 1.3 Run Cargo resolution and verify a single `swarm-p2p-core` version is used.

## 2. Bridge Types and Commands

- [ ] 2.1 Add mobile mirror enums/records for transfer phase, suspended reason, terminal reason, transfer projection, projection file, and save location.
- [ ] 2.2 Replace old history commands with projection list/detail commands.
- [ ] 2.3 Align accept/reject/pause/cancel/resume commands with shared core coordinator APIs.
- [ ] 2.4 Emit transfer projection update events through the mobile event bus.
- [ ] 2.5 Remove or quarantine old `MobileSessionStatus` history exports from new RN usage.

## 3. Artifact Generation

- [ ] 3.1 Regenerate TypeScript bindings.
- [ ] 3.2 Regenerate C++ bridge files.
- [ ] 3.3 Rebuild Android native artifacts.
- [ ] 3.4 Rebuild iOS artifacts if required by the package workflow.

## 4. Verification

- [ ] 4.1 Run mobile-core Rust build/check.
- [ ] 4.2 Run `pnpm typecheck`.
- [ ] 4.3 Run Android app/native-module load smoke.
- [ ] 4.4 Run `openspec validate sync-mobile-core-runtime --strict`.
