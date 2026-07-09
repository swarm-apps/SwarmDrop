## 1. Inventory and Boundaries

- [x] 1.1 Run a repository-wide inventory of `useXStore.getState/setState` calls and classify each as component, store-internal, external event bridge, router guard, synchronous utility, or test.
- [x] 1.2 Inventory desktop UI state currently stored outside routes and identify which transfer, inbox, device, send, and settings states must move to route/search params.
- [x] 1.3 Define the initial allowlist for legitimate command-style store access, including Tauri callbacks, router guards, synchronous utilities, and tests.

## 2. Route-Owned UI State

- [x] 2.1 Convert transfer detail navigation so the active session/detail target is owned by route path or search params and survives refresh/back-forward navigation.
- [x] 2.2 Convert inbox list/detail/search entry state so target item and filter/search state are route-owned where they are navigable or externally addressable.
- [x] 2.3 Convert device, send, and settings subpage entry points that currently rely on store or local pseudo-routing into standard route/search navigation.
- [x] 2.4 Update tray, external-open, transfer-complete, and other non-page entry points to navigate through standard routes instead of mutating store to imply navigation.

## 3. Zustand Usage Cleanup

- [x] 3.1 Replace component-level `useXStore.getState().action()` calls with action selectors in devices, send/share-target, transfer offer, close behavior, pairing, and related hooks.
- [x] 3.2 Replace component-level store snapshot reads with selector subscriptions or explicit one-shot props, using `useShallow` for multi-field selector results.
- [x] 3.3 Refactor store-internal self access so each store uses `create((set, get) => ...)` closures or explicit helper parameters instead of its exported hook API.
- [x] 3.4 Refactor cross-store orchestration that currently requires UI readback into domain helpers or action return values.

## 4. Enforcement and Validation

- [x] 4.1 Add a static allowlist check that fails on non-approved `useXStore.getState/setState` usages.
- [x] 4.2 Add or update focused tests for route-owned detail state, action return behavior, and store cleanup where behavior is non-trivial.
- [x] 4.3 Run desktop validation: typecheck, relevant unit tests, build, and `openspec validate standardize-state-routing-and-zustand`.
- [x] 4.4 Re-run the `getState/setState` inventory and confirm every remaining call is allowlisted and documented by boundary type.
