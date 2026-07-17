## 1. Inventory and Policy

- [x] 1.1 Run a repository-wide inventory of `useXStore.getState/setState` calls and classify each as component, store-internal, native event bridge, synchronous utility, or test.
- [x] 1.2 Define the initial allowlist for legitimate command-style store access, including `src/core/event-bus.ts`, synchronous utilities, and tests.
- [x] 1.3 Review persisted Zustand stores and identify durable fields versus runtime, error, queue, and legacy fields.

## 2. Component-Level Cleanup

- [x] 2.1 Replace component and hook level `useXStore.getState().action()` calls with action selectors in pairing request, settings network, transfer, inbox, and related components.
- [x] 2.2 Replace component snapshot reads with selector subscriptions or explicit action return values, using `useShallow` for multi-field selector results.
- [x] 2.3 Confirm Expo Router-owned navigation state remains route-owned and is not introduced into Zustand during cleanup.

## 3. Store Creator Cleanup

- [x] 3.1 Refactor `pairing-code-store` so ensure, regenerate, clear, markConsumed, timer scheduling, and async generation use closure `set/get` or explicit helper parameters.
- [x] 3.2 Add focused validation for pairing-code behavior boundaries through closure `set/get`, typecheck, lint, and static allowlist coverage; runtime tests are deferred until the mobile repo has a test runner.
- [x] 3.3 Review mobile-core and other stores for cross-store reads, and convert UI-facing orchestration to explicit action APIs where practical.

## 4. Action Results and Persistence

- [x] 4.1 Update node start/shutdown/restart flows so UI-triggered actions return explicit success or failure results instead of requiring immediate store readback.
- [x] 4.2 Add or tighten `partialize` for persisted stores so only durable preferences, onboarding state, and paired-device cache survive app restarts.
- [x] 4.3 Remove or migrate stale persisted fields that no longer represent active product behavior.

## 5. Enforcement and Validation

- [x] 5.1 Add a static allowlist check that fails on non-approved `useXStore.getState/setState` usages.
- [x] 5.2 Run mobile validation: lint, typecheck, static allowlist, and `openspec validate standardize-zustand-store-usage`.
- [x] 5.3 Re-run the `getState/setState` inventory and confirm every remaining call is allowlisted and documented by boundary type.
