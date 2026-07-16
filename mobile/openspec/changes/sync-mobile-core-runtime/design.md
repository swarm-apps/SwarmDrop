## Context

The current mobile bridge was created for the older history/resume API. Desktop now exposes `TransferProjection` as the frontend state source, backed by `TransferCoordinator`, epoch-based resume, and the transfer-data data plane. Trying to preserve `MobileSessionStatus` while adding projection fields would create two transfer models in RN.

## Goals / Non-Goals

**Goals:**

- Make the mobile bridge compile against the current shared SwarmDrop core.
- Export explicit typed mirror records for projections and related enums.
- Route projection updates as first-class RN events.
- Rebuild generated/native artifacts together.

**Non-Goals:**

- No compatibility shim for old history APIs.
- No UI redesign in this change; UI is handled by `redesign-mobile-foundation` and follow-up feature changes.
- No iOS runtime validation requirement.

## Decisions

### D1. Bridge projections directly

`mobile-core` will define mirror records for projection DTOs instead of converting into old history items. RN will derive all Activity views from these projections.

### D2. Keep generated artifacts in the same change

The native API checksum changes when bridge methods or records change. Generated TS, C++, Android, and iOS artifacts must move together to avoid stale linker/runtime failures.

### D3. Android is the runtime smoke target

The required mobile runtime validation is Android only. iOS artifacts should still be generated if required by the package workflow, but runtime testing is not part of this change.

## Risks / Trade-offs

- [Risk] Shared core API drift during implementation. -> Pin to a concrete SwarmDrop commit or tag before regenerating artifacts.
- [Risk] Generated files hide stale native outputs. -> Verify checksums and run a narrow Android build/load smoke.
- [Risk] Removing old history APIs breaks existing screens. -> Pair with `redesign-mobile-foundation` before shipping.

## Migration Plan

1. Pin the mobile bridge to the target SwarmDrop revision.
2. Replace bridge records and commands.
3. Regenerate bindings/artifacts.
4. Update RN event/type imports.
5. Run checks and Android smoke validation.
