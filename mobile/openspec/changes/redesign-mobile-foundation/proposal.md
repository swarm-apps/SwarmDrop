## Why

SwarmDrop desktop has moved from a simple transfer-history product into a trusted data channel with explicit transfer lifecycle projections, Drop Inbox, per-device trust policies, and LAN helper discovery. The mobile app still presents the old device/history model, so it needs a foundation rewrite before inbox and trust can land cleanly.

## What Changes

- **BREAKING**: Replace the mobile transfer model based on `MobileSessionStatus` and `dbHistory` with a projection-first model based on `TransferProjection` phase/reason/epoch/recoverable semantics from the shared core.
- **BREAKING**: Redesign the primary mobile information architecture around Devices, Inbox, and Activity instead of the current drawer-first Home + Transfer History flow.
- Add a mobile Drop Inbox surface for successfully received content, separate from transfer activity and recovery.
- Add a mobile Activity and Recovery surface for active transfers, recoverable suspended transfers, failures, cancellations, policy decisions, and completed diagnostics.
- Add device trust foundations: trust badges on device cards, a device detail surface, and a bottom-sheet policy editor shaped for mobile.
- Replace global auto-accept as the main UX with per-device receive policy decisions. Existing global preferences may be removed or demoted during implementation.
- Add mobile network discovery settings and status for automatic public bootstrap, LAN helper discovery, and Android-friendly status inspection.
- Update mobile native bridge mirrors and generated bindings to track the latest shared SwarmDrop core APIs. Compatibility with the older `add-transfer-history` bridge is not required.
- Define Android-only validation using Maestro flows for the new navigation, inbox/activity/device trust, and transfer offer surfaces.

## Capabilities

### New Capabilities

- `mobile-shell-navigation`: Mobile-first primary navigation, app shell, empty states, and visual foundation for Devices, Inbox, Activity, and Settings.
- `mobile-transfer-projections`: Projection-first transfer store, status mapping, event handling, and Activity/Recovery behavior.
- `mobile-drop-inbox`: Mobile Drop Inbox list, detail, content actions, deletion semantics, and receive-completion visibility.
- `mobile-device-trust`: Mobile trust badges, device detail, trust-level selection, per-device receive policy editing, and policy-gated receive presentation.
- `mobile-network-discovery`: Mobile network discovery preferences and status surfaces for automatic bootstrap, LAN helper discovery, and relay readiness.

### Modified Capabilities

None. The repo currently has no archived baseline specs; the existing `add-transfer-history` change is superseded by this foundation redesign.

## Impact

- `packages/swarmdrop-core/rust/mobile-core`: sync dependencies to the latest SwarmDrop shared core, replace old history bridge types, add mirror records/enums for projections, inbox, trust policy, and network discovery.
- `packages/swarmdrop-core/src/generated`, `cpp/generated`, iOS xcframework, and Android artifacts: regenerate after native bridge changes.
- `src/stores`: replace transfer/history state, extend device/network/preferences state, and keep host-specific logic thin.
- `src/app`: restructure primary navigation and add mobile Inbox, Activity, Device Detail, and policy/settings routes or sheets.
- `src/components`: introduce reusable mobile primitives for status badges, trust badges, device rows, activity rows, inbox rows, policy sheets, and network status blocks.
- `src/locales`: update zh-Hans and en strings for new information architecture and policy/status language.
- Validation: `pnpm typecheck`, relevant Rust/mobile-core checks, generated binding checks, and Android-only Maestro flows.
