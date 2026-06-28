## Context

SwarmDrop-RN currently has a drawer-first mobile shell with a Home screen, a "Transfer History" screen, and a transfer store shaped around `sessions + dbHistory`. Desktop SwarmDrop has moved to transfer projections, Drop Inbox, device trust policies, and richer network discovery. Those vertical features should not all be built inside one broad UI change, but the mobile app still needs a clean shell and domain-state foundation before they can be implemented elegantly.

This change is therefore the UI foundation layer. It assumes `sync-mobile-core-runtime` has already exposed transfer projection types and projection update events. It creates the app shell, projection-first Activity, shared primitives, and placeholder/domain boundaries for Inbox, Trust, and Network. The focused follow-up changes fill in the native APIs and full behaviors.

## Goals / Non-Goals

**Goals:**

- Make Devices, Inbox, and Settings the primary mobile surfaces.
- Move Activity into a secondary transfer process page reachable from Devices and transfer detail flows.
- Keep Settings out of per-screen top-right shortcuts; it has its own primary tab.
- Make transfer UI projection-first and remove UI dependence on old history/status filters.
- Create a small set of reusable mobile primitives instead of styling each screen independently.
- Establish Inbox, trust, and network state/screen boundaries that later changes can extend without another shell rewrite.
- Keep implementation simple: one domain store per product domain and no duplicated history/projection state.

**Non-Goals:**

- No compatibility shim for old `MobileSessionStatus` history APIs.
- No native Inbox API implementation; that belongs to `add-mobile-drop-inbox`.
- No persisted trust-policy editing; that belongs to `add-mobile-device-trust-policies`.
- No native network discovery runtime/status sync; that belongs to `sync-mobile-network-discovery`.
- No complete Android Maestro suite; that belongs to `add-android-maestro-validation`.
- No iOS validation requirement in this change.

## Decisions

### D1. Apply after native projection sync

`sync-mobile-core-runtime` is the hard prerequisite. The foundation should import generated projection types and commands rather than define local compatibility DTOs. This keeps the UI rewrite honest and avoids a second transfer model.

### D2. Replace drawer-first navigation with a bottom-tab primary shell

The mobile app will use three primary tabs:

```text
Devices      Inbox                    Settings
  |            |                         |
send/pair   received                  app/network/
trust       content                   appearance/device
  |
Activity (secondary transfer process/recovery/diagnostics)
```

Settings is a primary tab rather than a repeated top-right shortcut. Activity is intentionally secondary so the primary shell stays focused on devices, received content, and app configuration.

### D3. Use projection-first transfer state as the only runtime source

The transfer store will keep:

```ts
projections: Record<string, MobileTransferProjection>
progressBySession: Record<string, MobileTransferProgress>
pendingOffers: MobileTransferOffer[]
```

`dbHistory` and old history status filters are removed from new UI consumers. Activity groups are derived from projection phase/reason/recoverable fields.

### D4. Build feature boundaries before vertical feature depth

Inbox, Trust, and Network need visible homes and stable state boundaries in this change, but their full native behavior should remain in focused follow-up changes. That keeps each OpenSpec change reviewable and lets every completed change run its own dev-workflow closeout.

### D5. Use restrained app-native mobile primitives

The foundation uses compact headers, stable tab dimensions, 44 pt touch targets, 8 point spacing, status/trust badges, and bottom-sheet action areas. Components should serve repeated workflows, not become decorative cards nested inside cards.

## Migration Plan

1. Treat `add-transfer-history` as superseded for new mobile transfer UI.
2. Apply `sync-mobile-core-runtime` first and regenerate projection bindings.
3. Replace root mobile navigation with the bottom-tab shell.
4. Replace the transfer store and Activity consumers with projection-first state.
5. Add Inbox, Device Detail, secondary Activity, and Settings/Network boundaries using shared primitives.
6. Update copy, test IDs, and validation for the foundation.

Rollback during development is branch-level. Compatibility with the old drawer/history model is not maintained.
