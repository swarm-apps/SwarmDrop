## Why

The mobile redesign and native bridge sync create many user-facing paths that are easy to regress. Android is the required validation target, and Maestro gives repeatable checks for navigation and state presentation without requiring iOS coverage.

## What Changes

- Add Android-only Maestro flows for the new mobile shell, Devices, Inbox, Activity, device trust, network settings, and receive offer presentation.
- Add stable `testID`s needed by Maestro across primary screens and critical controls.
- Document the local Android validation command and artifact expectations.
- Keep real cross-device throughput validation separate from UI automation.

## Capabilities

### New Capabilities

- `android-maestro-mobile-validation`: Android Maestro validation suite for SwarmDrop-RN primary mobile flows.

### Modified Capabilities

None. This is a validation capability.

## Impact

- Maestro flow files under the repo's test/e2e convention.
- `testID` additions in RN components and screens.
- Dev notes for running Android-only validation.
- Optional fixture/mock entry points where the app already supports deterministic state.
