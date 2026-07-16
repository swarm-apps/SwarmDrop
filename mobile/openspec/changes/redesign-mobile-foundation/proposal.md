## Why

SwarmDrop desktop has moved from a simple transfer-history product into a trusted data channel with explicit transfer lifecycle projections, Drop Inbox, per-device trust policies, and LAN helper discovery. The mobile app still presents the old device/history model, so it needs a UI and state foundation before the focused Inbox, trust-policy, and network-discovery changes can land cleanly.

## What Changes

- **BREAKING**: Redesign the primary mobile information architecture around Devices, Inbox, and Settings instead of the current drawer-first Home + Transfer History flow, with Activity as a secondary transfer process page.
- **BREAKING**: Replace mobile UI usage of `MobileSessionStatus`/`dbHistory` with projection-first Activity state that consumes the bridge from `sync-mobile-core-runtime`.
- Add shared mobile primitives for the new app shell, headers, rows/cards, badges, empty states, and bottom-sheet action areas.
- Add the foundational Devices, Inbox, and Settings tab surfaces plus a secondary Activity surface:
  - Devices becomes the sending and pairing home.
  - Inbox becomes the received-content destination surface, initially wired as a store/screen boundary for `add-mobile-drop-inbox`.
  - Settings becomes a primary tab for app, network, appearance, and device preferences.
  - Activity becomes the secondary transfer process and recovery surface powered by transfer projections.
- Add device trust UI foundations: trust badge defaults, device detail shell, and policy-editor entry points. Full policy persistence is implemented by `add-mobile-device-trust-policies`.
- Add network discovery UI foundations in Settings. Runtime discovery config and richer status fields are implemented by `sync-mobile-network-discovery`.
- Add stable Android test IDs for the new primary shell. The full Android Maestro suite is implemented by `add-android-maestro-validation`.

## Capabilities

### New Capabilities

- `mobile-shell-navigation`: Mobile-first primary navigation, app shell, empty states, and visual foundation for Devices, Inbox, secondary Activity, and Settings.
- `mobile-transfer-projections`: Projection-first transfer store, status mapping, event handling, and Activity/Recovery behavior.
- `mobile-drop-inbox`: Foundational mobile Inbox tab and state boundary for received content.
- `mobile-device-trust`: Foundational mobile trust badges, device detail shell, and policy editor entry points.
- `mobile-network-discovery`: Foundational mobile network discovery settings placement and status presentation slots.

### Modified Capabilities

None. The repo currently has no archived baseline specs; the existing `add-transfer-history` change is superseded by the projection-first mobile foundation.

## Dependencies

- Must be applied after `sync-mobile-core-runtime`, because Activity and transfer store code should consume generated transfer projection types rather than invent local compatibility shims.
- Follow-up vertical slices:
  - `add-mobile-drop-inbox` implements native Inbox APIs, real list/detail actions, and destructive content semantics.
  - `add-mobile-device-trust-policies` implements persisted trust policy editing and policy-gated receive behavior.
  - `sync-mobile-network-discovery` implements native runtime config/status fields and full network settings behavior.
  - `add-android-maestro-validation` implements the complete Android validation suite.

## Impact

- `src/stores`: replace transfer/history state with projection-first state, add lightweight Inbox/trust/network state boundaries, and keep host-specific logic thin.
- `src/app`: restructure primary navigation and add mobile Inbox, secondary Activity, Device Detail, and Settings tab surfaces.
- `src/components`: introduce reusable mobile primitives for status badges, trust badges, device rows, activity rows, inbox rows, bottom sheets, and status blocks.
- `src/locales`: update zh-Hans and en strings for the new information architecture and projection/status language.
- Validation: `pnpm typecheck`, lint/Biome check, Android shell smoke where practical, and `openspec validate redesign-mobile-foundation --strict`.
