## Context

SwarmDrop-RN currently has a drawer-first mobile shell with a Home screen, a "Transfer History" screen, and a bridge pinned to the desktop `v0.5.3` core. The current transfer store keeps `sessions + dbHistory` and maps to the old five-state `MobileSessionStatus` model. Desktop SwarmDrop has since changed the product model:

- transfer state is represented by `TransferProjection { phase, reason, epoch, recoverable }`;
- successfully received content is indexed through Drop Inbox;
- active, suspended, failed, cancelled, and diagnostic transfer state belongs to Activity and Recovery;
- paired devices carry trust levels and per-device receive policy;
- network discovery is driven by runtime discovery options and LAN helper candidates.

This change intentionally does not preserve the old mobile history bridge or old drawer IA. Compatibility with `add-transfer-history` is not a goal; a simpler and cleaner foundation is cheaper than layering translation code over the old model.

## Goals / Non-Goals

**Goals:**

- Make Devices, Inbox, and Activity the primary mobile surfaces.
- Keep transfer state projection-first and host-neutral, with native bridge mirrors only at the RN boundary.
- Separate content operations from transfer process diagnostics.
- Put trust and policy controls on device-centric surfaces, not global settings.
- Keep mobile UI ergonomic: thumb-zone primary actions, bottom sheets for task-specific controls, clear empty states, and stable 44 pt tap targets.
- Keep implementation simple: one domain store per product domain and no duplicated history/projection state.
- Validate the first implementation on Android with Maestro flows.

**Non-Goals:**

- No backward compatibility for old `MobileSessionStatus` history APIs or stale generated bindings.
- No iOS validation requirement in this change.
- No cloud sync, account identity, or cross-device inbox sync.
- No Android foreground transfer service or local LAN helper server mode in this foundation pass.
- No full MCP permission UI beyond storing/presenting the receive-policy fields already needed for trusted receive.

## Decisions

### D1. Replace drawer-first navigation with a bottom-tab primary shell

The mobile app will use three primary tabs:

```text
Devices      Inbox       Activity
  |            |            |
send/pair   received     active/recoverable/diagnostic
trust       content      transfer process
```

Settings remains a secondary screen from the top bar or a sheet. This matches mobile usage: sending starts from devices, receiving ends in inbox, and recovery/diagnostics live in activity. A drawer hides the new Inbox too deeply and makes Activity look like old history.

Alternative considered: keep Drawer and add Inbox as another drawer item. Rejected because the product now has three primary mobile tasks, and drawer navigation is slower for repeated phone workflows.

### D2. Use projection-first transfer state as the only runtime source

The RN transfer store will keep:

```ts
projections: Record<string, MobileTransferProjection>
progressBySession: Record<string, MobileTransferProgress>
pendingOffers: MobileTransferOffer[]
```

`dbHistory` and old history status filters are removed. Activity groups are derived from projections:

- active: `offered | waiting_accept | active`
- recoverable: `suspended && recoverable`
- attention: non-recoverable suspended plus terminal failures/cancellations/rejections
- completed diagnostics: terminal completed, shown as process context only

Alternative considered: keep the old `sessions + dbHistory` store and translate projections into old statuses. Rejected because it creates two state models and hides the exact suspended/terminal reasons needed for recovery UX.

### D3. Put Drop Inbox in its own store and screen

Inbox is not renamed history. It is a content index with list/detail/actions. It uses separate native APIs and state:

```ts
items: MobileInboxItemSummary[]
selectedItem: MobileInboxItemDetail | null
includeArchived: boolean
```

Clearing Activity must not remove inbox items. Deleting an inbox item must have explicit content semantics: remove record only, or also delete local files if the platform operation is supported and confirmed.

Alternative considered: show completed receive projections as the user's received files. Rejected because projections are process records; they do not own content actions, missing-file state, archiving, or delete-local-files confirmation.

### D4. Make device trust a device-detail responsibility

Device cards show online/offline, connection path, and trust badge. Tapping a paired device opens device detail. Sending remains a primary action, but trust editing is in a bottom sheet from device detail:

```text
Device Detail
  - identity and connection
  - trust badge and policy summary
  - Send files
  - Edit receive policy
  - Block / unblock
```

The policy editor uses mobile-first controls: segmented trust selection, switches for booleans, numeric input for size limit, and advanced fields collapsed by default. The default sheet is concise; expert knobs do not dominate the screen.

Alternative considered: copy the desktop dialog as a dense form. Rejected because it is too heavy for phone ergonomics and makes trust feel like a settings chore instead of a device relationship.

### D5. Keep native bridge mirrors explicit and disposable

`mobile-core` will mirror the shared core types needed by RN:

- `MobileTransferProjection`, phase/reason enums, projection files, policy context fields;
- `MobileInboxItemSummary`, `MobileInboxItemDetail`, `MobileInboxItemFileEntry`;
- `MobileDeviceTrustLevel`, `MobileDeviceReceivePolicy`;
- `MobileNetworkRuntimeConfig`, `MobileNetworkStatus` discovery fields.

The bridge can be rewritten without compatibility shims. Generated TypeScript and native artifacts must be regenerated after bridge changes. The shared core remains the business source of truth; RN bridge code only converts host-neutral records.

Alternative considered: export JSON blobs to reduce mirror work. Rejected because this would weaken type safety exactly where breaking native changes are most likely.

### D6. Network discovery UI controls discovery, not mobile relay hosting

Mobile should support automatic public bootstrap and LAN helper discovery. It should show candidate counts, relay readiness, and helper source. It should not make the phone a default LAN helper/relay server in this foundation change because mobile OS background and battery constraints require a separate lifecycle design.

Implementation may still pass `provideLanHelper=false` through runtime config so the bridge stays aligned with shared core, but the primary mobile UI only exposes discovery mode and auto-discovery.

Alternative considered: expose all desktop network settings 1:1. Rejected because desktop can be a stable infrastructure node, while a phone usually cannot.

### D7. Android Maestro is the implementation acceptance loop

Implementation tasks must add stable `testID`s for the main Android flows and validate with Maestro on Android only. Maestro coverage should focus on navigation and state presentation, not real cross-device throughput. Transfer networking can use existing local/manual validation where real devices are required.

## Risks / Trade-offs

- [Risk] Native bridge sync touches Rust, generated TS, and native artifacts. -> Mitigation: do bridge work in one explicit task group, rebuild bindings/artifacts, and run `pnpm typecheck` before UI work proceeds.
- [Risk] Bottom tabs can overcrowd small screens. -> Mitigation: limit primary tabs to three labels and move Settings to a top action.
- [Risk] Inbox and Activity may still feel similar. -> Mitigation: copy and empty states must describe content vs process, and clear/delete commands must be physically separated.
- [Risk] Policy editing is powerful enough to be dangerous. -> Mitigation: default existing/unknown devices to collaborator semantics, use visible trust badges, and require explicit destructive confirmation for blocked/unblock changes.
- [Risk] Android Maestro cannot prove P2P correctness. -> Mitigation: use Maestro for UI regressions and keep Rust/core tests plus manual two-device transfer validation for network behavior.

## Migration Plan

1. Archive or stop implementing `add-transfer-history`; use this change as the new implementation line.
2. Sync `packages/swarmdrop-core/rust/mobile-core` to the latest shared SwarmDrop core and replace old history bridge exports with projection/inbox/trust/network exports.
3. Regenerate TypeScript and native binding artifacts.
4. Replace root mobile navigation with the bottom-tab shell and route/settings structure.
5. Implement projection, inbox, device trust, and network discovery stores.
6. Implement Devices, Inbox, Activity, Device Detail, policy sheets, and network discovery settings.
7. Add Android Maestro flows and run them after `pnpm typecheck`.

Rollback strategy during development: revert the branch or change before release. Because compatibility is not required and generated artifacts are rebuilt together, partial runtime rollback is not supported.

## Open Questions

- Should Inbox be the initial tab after onboarding, or should Devices remain the initial tab? Initial recommendation: Devices for new users, last-selected tab for returning users after the first successful receive.
- Should a future Android foreground service enable local LAN helper mode? This should be a separate change because it changes battery, notification, and background behavior.
