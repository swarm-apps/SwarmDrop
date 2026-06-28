## 1. Baseline and Cutover

- [ ] 1.1 Record that `add-transfer-history` is superseded and stop using it as the implementation source for mobile transfer UI.
- [ ] 1.2 Review the latest SwarmDrop desktop `develop` bindings for transfer projections, inbox DTOs, trusted device policies, and network discovery fields.
- [ ] 1.3 Decide the implementation branch strategy for the breaking native bridge sync and confirm generated/native artifacts will be updated together.
- [ ] 1.4 Remove or quarantine old assumptions in dev notes that describe Transfer History as the primary received-content surface.

## 2. Native Bridge Sync

- [ ] 2.1 Update `packages/swarmdrop-core/rust/mobile-core/Cargo.toml` to depend on the latest shared SwarmDrop core revision needed by transfer projections, inbox, trust policy, and network discovery.
- [ ] 2.2 Replace old `MobileSessionStatus` history exports with `MobileTransferProjection`, phase/reason enums, projection files, and projection list/detail APIs.
- [ ] 2.3 Add mobile bridge records and APIs for inbox item summaries, inbox detail, inbox files, archive/delete/repair/missing-file operations.
- [ ] 2.4 Add mobile bridge records and APIs for device trust level, receive policy, trust-confirmed state, and paired-device policy updates.
- [ ] 2.5 Extend mobile network startup/status bridge types with discovery mode, auto-discover LAN helpers, candidate source counts, relay source, LAN helper count, and local helper state.
- [ ] 2.6 Route shared core transfer projection events, policy-gated offer context, inbox-relevant receive completion, device policy updates, and network status changes into the RN event bus.
- [ ] 2.7 Regenerate TypeScript bindings, C++ generated files, Android native artifacts, and iOS artifacts as required by the ubrn build pipeline.
- [ ] 2.8 Run mobile-core Rust build/check commands and confirm generated binding checksums match the native module.

## 3. Domain Stores

- [ ] 3.1 Replace `src/stores/transfer-store.ts` with a projection-first store: `projections`, `progressBySession`, and `pendingOffers`.
- [ ] 3.2 Add transfer projection helpers for active/recoverable/attention/completed grouping, status labels, progress merge, resume eligibility, and policy reason display.
- [ ] 3.3 Add an inbox store for list/detail/loading/archive/delete/repair/missing-file refresh behavior.
- [ ] 3.4 Extend the mobile core/device store to preserve trust and receive-policy fields for both online devices and offline paired-device cache.
- [ ] 3.5 Extend preferences/network state for discovery mode, auto-discover LAN helpers, and advanced custom bootstrap nodes.
- [ ] 3.6 Remove obsolete `dbHistory` actions and old status-filter assumptions from store consumers.

## 4. Mobile Shell Navigation

- [ ] 4.1 Replace the drawer-first authenticated shell with bottom tabs for Devices, Inbox, and Activity.
- [ ] 4.2 Move Settings to a secondary top action or modal route outside the three primary tabs.
- [ ] 4.3 Add stable test IDs for each tab, top-level screen, and empty state.
- [ ] 4.4 Update root routing and onboarding redirect logic to enter the new shell after setup.
- [ ] 4.5 Ensure primary tab labels, icons, safe-area padding, and tap targets work on 375px Android width.

## 5. Visual Foundation Components

- [ ] 5.1 Create shared mobile primitives for app header, tab screen container, status badge, trust badge, row/card surface, empty state, and bottom-sheet action area.
- [ ] 5.2 Normalize transfer status colors and copy around projection phase/reason rather than old flat statuses.
- [ ] 5.3 Normalize inbox item visuals around content kind, source, received time, missing state, and archived state.
- [ ] 5.4 Normalize device visuals around trust level, connection path, online state, and primary send action.
- [ ] 5.5 Audit new components for 44 pt touch targets, 8 point spacing, text truncation, and accessible contrast.

## 6. Devices and Trust UI

- [ ] 6.1 Build the Devices tab with paired devices, nearby/pairing action, node status summary, and active transfer summary.
- [ ] 6.2 Change paired-device tap behavior to open Device Detail instead of immediately opening file selection.
- [ ] 6.3 Build Device Detail with identity, connection status, trust summary, policy summary, send action, and management actions.
- [ ] 6.4 Build the mobile policy editor bottom sheet with trust segmented control, policy switches, size limit input, save location field, expiration field, and advanced section.
- [ ] 6.5 Implement owned/collaborator/temporary/blocked default policy templates in RN helpers aligned with shared core defaults.
- [ ] 6.6 Wire policy save/block/unblock/unpair actions to native commands and refresh device state after success.
- [ ] 6.7 Update incoming offer presentation to show policy auto-accept, require-confirmation, and policy-rejected context.

## 7. Inbox UI

- [ ] 7.1 Build the Inbox tab list with item title, source, item count, total size, received time, content kind, archived/missing state, and empty state.
- [ ] 7.2 Build Inbox detail with file list, source metadata, linked transfer diagnostics, and missing-file indicators.
- [ ] 7.3 Implement mobile content actions: open/share file, copy URI/path, archive/unarchive item, delete record, and optional delete-local-files confirmation when supported.
- [ ] 7.4 Ensure Inbox refreshes after receive completion, archive/delete, missing-file marking, and repair operations.
- [ ] 7.5 Ensure clearing Activity does not remove Inbox items or local received files.

## 8. Activity and Recovery UI

- [ ] 8.1 Build the Activity tab with Active, Recoverable, Needs Attention, and Completed Diagnostics sections.
- [ ] 8.2 Build projection rows/cards with peer, direction, files, progress, phase/reason label, policy action/reason, timestamps, and primary action.
- [ ] 8.3 Build transfer detail from projection data with progress, file projection list, status explanation, policy context, resume/cancel/pause/delete actions, and links to Inbox when relevant.
- [ ] 8.4 Wire pause, cancel, resume, delete transfer record, and clear Activity actions to native commands.
- [ ] 8.5 Remove old transfer history filters based on `MobileSessionStatus`.

## 9. Network Discovery UI

- [ ] 9.1 Update node startup to pass mobile network runtime config with discovery mode, LAN helper discovery, custom bootstrap nodes, and `provideLanHelper=false` by default.
- [ ] 9.2 Redesign Network settings around discovery mode and LAN helper discovery, with manual bootstrap nodes in an advanced section.
- [ ] 9.3 Update node status surfaces to show bootstrap readiness, relay readiness, LAN helper count, candidate sources, relay source, and connected/discovered peers.
- [ ] 9.4 Show restart-required hints when changing discovery settings while the node is running.
- [ ] 9.5 Keep helper hosting hidden or advanced-only and defaulted off.

## 10. Internationalization and Copy

- [ ] 10.1 Update zh-Hans and en copy for Devices, Inbox, Activity, trust levels, receive policy, network discovery, and projection reasons.
- [ ] 10.2 Remove or rewrite old "传输历史" copy that now refers to Activity or Inbox.
- [ ] 10.3 Run Lingui extraction and ensure no missing messages for zh-Hans or en.

## 11. Android Maestro Validation

- [ ] 11.1 Add Maestro flow for Android primary navigation: launch app, open Devices, Inbox, Activity, and Settings.
- [ ] 11.2 Add Maestro flow for Android empty states: no paired devices, empty Inbox, and empty Activity.
- [ ] 11.3 Add Maestro flow for Android device trust UI: open device detail, open policy editor, switch trust level, and dismiss without saving.
- [ ] 11.4 Add Maestro flow for Android network settings: open discovery settings and advanced bootstrap section.
- [ ] 11.5 Add Maestro flow for Android incoming offer presentation using fixture/mockable state where available.
- [ ] 11.6 Document that iOS Maestro/manual validation is not required for this change.

## 12. Verification

- [ ] 12.1 Run `pnpm typecheck`.
- [ ] 12.2 Run `pnpm lint` or the repo's current Biome check.
- [ ] 12.3 Run mobile-core Rust build/check commands for the bridge crate.
- [ ] 12.4 Run Android native build or the narrowest Android build command that proves regenerated artifacts load.
- [ ] 12.5 Run the Android Maestro flows from section 11 on an Android emulator or device.
- [ ] 12.6 Run `openspec validate redesign-mobile-foundation --strict`.
- [ ] 12.7 Manually smoke-test one Android happy path with a real or paired peer when available: device list, send entry, incoming receive, Inbox visibility, and Activity update.
