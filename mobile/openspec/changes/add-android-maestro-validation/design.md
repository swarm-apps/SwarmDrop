## Context

SwarmDrop transfers often require two real endpoints, but many mobile UI regressions can be caught with single-device Android flows. Maestro should validate navigation and visible states, while core networking remains covered by Rust tests and manual paired-device checks.

## Goals / Non-Goals

**Goals:**

- Provide repeatable Android Maestro flows for the new primary UX.
- Require stable test IDs on key elements.
- Produce useful screenshots/logs for debugging.

**Non-Goals:**

- No iOS Maestro flows.
- No full P2P throughput automation in this change.
- No cloud Maestro requirement unless local devices are unavailable.

## Decisions

### D1. Validate surfaces, not network physics

Maestro flows cover shell navigation, empty states, policy editor visibility, network settings, and offer presentation. Real transfer correctness remains outside this test suite.

### D2. Use stable test IDs

Text copy can change with i18n. Maestro should prefer explicit test IDs for tabs, screens, rows, sheets, and destructive confirmations.

### D3. Android only

The suite targets Android emulator/device. iOS flows are explicitly out of scope for this change.

## Risks / Trade-offs

- [Risk] No deterministic fixture for offer/inbox states. -> Add lightweight debug or fixture hooks only if the app already has a safe pattern; otherwise keep those flows manual/partial.
- [Risk] Maestro becomes flaky during native startup. -> Wait on stable screen test IDs and node status IDs instead of arbitrary sleeps.

## Migration Plan

1. Add test IDs during UI implementation.
2. Add Maestro flows incrementally by surface.
3. Document commands.
4. Run locally on Android and keep artifacts for debugging.
