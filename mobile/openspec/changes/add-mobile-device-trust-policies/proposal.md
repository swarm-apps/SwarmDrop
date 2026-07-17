## Why

Desktop SwarmDrop now uses device trust and per-device receive policies as the trust boundary for automatic receive. Mobile still has a global auto-accept preference, which is too blunt for inbox-based trusted receiving.

## What Changes

- Add trust level and receive policy to mobile paired device data.
- Add trust badges and device detail policy summaries.
- Add mobile policy editor for owned, collaborator, temporary, and blocked devices.
- Replace global auto-accept behavior with policy-gated receive presentation.
- Add Android Maestro validation for device detail and policy editor.

## Capabilities

### New Capabilities

- `mobile-device-policy-management`: Mobile device detail, trust badges, policy editor, and policy persistence.
- `mobile-policy-gated-receive`: Incoming offer UX that reflects automatic accept, confirmation, and policy rejection.

### Modified Capabilities

None. No archived mobile trust policy capability exists.

## Impact

- `mobile-core`: trust level / receive policy mirrors and update command.
- `src/stores/mobile-core-store.ts`: paired device cache includes policy fields.
- `src/components` and `src/app`: device detail, trust badge, policy sheet, incoming offer policy copy.
- `src/stores/preferences-store.ts`: remove or demote global auto-accept.
