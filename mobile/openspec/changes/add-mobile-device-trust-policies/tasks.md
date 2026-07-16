## 1. Bridge and Store

- [x] 1.1 Add mobile trust level and receive policy mirror records.
- [x] 1.2 Expose update paired device policy command.
- [x] 1.3 Persist trust/policy fields in offline paired device cache.
- [x] 1.4 Remove or demote global auto-accept from primary receive behavior.

## 2. Device Surfaces

- [x] 2.1 Add trust badge component.
- [x] 2.2 Build paired device detail surface.
- [x] 2.3 Disable send actions for blocked devices.
- [x] 2.4 Add policy summary to device detail.

## 3. Policy Editor

- [x] 3.1 Build bottom-sheet policy editor.
- [x] 3.2 Add trust default template helpers.
- [x] 3.3 Wire save/block/unblock/unpair actions.
- [x] 3.4 Refresh device list/cache after policy updates.

## 4. Receive UX

- [x] 4.1 Show policy context in manual offer dialog.
- [x] 4.2 Handle auto-accepted offers in Activity without manual prompt.
- [x] 4.3 Handle policy-rejected offers with clear copy.

## 5. Validation

- [x] 5.1 Add Android Maestro flow for device detail and policy editor.
- [x] 5.2 Run `pnpm typecheck`.
- [x] 5.3 Run `openspec validate add-mobile-device-trust-policies --strict`.
