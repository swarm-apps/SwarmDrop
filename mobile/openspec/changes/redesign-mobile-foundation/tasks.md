## 1. Baseline and Cutover

- [x] 1.1 Record that `add-transfer-history` is superseded for new mobile transfer UI.
- [x] 1.2 Confirm `sync-mobile-core-runtime` has been applied before consuming generated transfer projection types.
- [x] 1.3 Remove or quarantine dev-note assumptions that describe Transfer History as the primary received-content surface.

## 2. Projection-First Domain State

- [x] 2.1 Replace `src/stores/transfer-store.ts` with a projection-first store: `projections`, `progressBySession`, and `pendingOffers`.
- [x] 2.2 Add transfer projection helpers for active/recoverable/attention/completed grouping, status labels, progress merge, resume eligibility, and policy reason display.
- [x] 2.3 Remove obsolete `dbHistory` actions and old status-filter assumptions from new store consumers.
- [x] 2.4 Add lightweight Inbox state boundary for empty/loading/refresh placeholders without implementing native Inbox actions.
- [x] 2.5 Add lightweight trust UI helpers for trust badge defaults and policy-summary placeholders.
- [x] 2.6 Add lightweight network discovery preference/status boundary that follow-up network sync can extend.

## 3. Mobile Shell Navigation

- [x] 3.1 Replace the drawer-first authenticated shell with bottom tabs for Devices, Inbox, and Settings.
- [x] 3.2 Move Activity to a secondary transfer process page reachable from Devices and transfer flows.
- [x] 3.3 Add stable test IDs for each tab, top-level screen, and empty state.
- [x] 3.4 Update root routing and onboarding redirect logic to enter the new shell after setup.
- [x] 3.5 Ensure primary tab labels, icons, safe-area padding, and tap targets work on 375px Android width.

## 4. Visual Foundation Components

- [x] 4.1 Create shared mobile primitives for app header, tab screen container, status badge, trust badge, row/card surface, empty state, and bottom-sheet action area.
- [x] 4.2 Normalize transfer status colors and copy around projection phase/reason rather than old flat statuses.
- [x] 4.3 Normalize inbox placeholder visuals around content kind, source, received time, missing state, and archived state.
- [x] 4.4 Normalize device visuals around trust level, connection path, online state, and primary send action.
- [x] 4.5 Audit new components for 44 pt touch targets, 8 point spacing, text truncation, and accessible contrast.

## 5. Foundation Screens

- [x] 5.1 Build the Devices tab with paired devices, nearby/pairing action, node status summary, and active transfer summary.
- [x] 5.2 Change paired-device tap behavior to open Device Detail instead of immediately opening file selection.
- [x] 5.3 Build Device Detail shell with identity, connection status, trust summary placeholder, send action, and management action slots.
- [x] 5.4 Build the Inbox tab empty/loading/list shell that can later bind to native Inbox records.
- [x] 5.5 Build the secondary Activity page with Active, Recoverable, Needs Attention, and Completed Diagnostics sections.
- [x] 5.6 Build projection rows/cards with peer, direction, files, progress, phase/reason label, policy action/reason, timestamps, and primary action.
- [x] 5.7 Build Network settings foundation with discovery controls placement and advanced bootstrap section placement.

## 6. Internationalization and Copy

- [x] 6.1 Update zh-Hans and en copy for Devices, Inbox, Activity, trust labels, network discovery placeholders, and projection reasons.
- [x] 6.2 Remove or rewrite old "传输历史" copy that now refers to Activity or Inbox.
- [x] 6.3 Run Lingui extraction and ensure no missing messages for zh-Hans or en.

## 7. Android Foundation Validation

- [x] 7.1 Add or update Android smoke flow for primary navigation: launch app, open Devices, Inbox, Settings, and secondary Activity.
- [x] 7.2 Add Android smoke coverage for empty states where fixture data is unavailable.
- [x] 7.3 Document that full device trust, Inbox detail/delete, network discovery, and receive-offer Maestro coverage belongs to follow-up validation changes.

## 8. Verification

- [x] 8.1 Run `pnpm typecheck`.
- [x] 8.2 Run `pnpm lint` or the repo's current Biome check.
- [x] 8.3 Run Android native build or the narrowest Android command that proves the app still starts with regenerated core artifacts.
- [x] 8.4 Run the Android foundation Maestro smoke flow when an Android device/emulator is available.
- [x] 8.5 Run `openspec validate redesign-mobile-foundation --strict`.
