## 1. Bridge and Store

- [ ] 1.1 Add mobile trust level and receive policy mirror records.
- [ ] 1.2 Expose update paired device policy command.
- [ ] 1.3 Persist trust/policy fields in offline paired device cache.
- [ ] 1.4 Remove or demote global auto-accept from primary receive behavior.

## 2. Device Surfaces

- [ ] 2.1 Add trust badge component.
- [ ] 2.2 Build paired device detail surface.
- [ ] 2.3 Disable send actions for blocked devices.
- [ ] 2.4 Add policy summary to device detail.

## 3. Policy Editor

- [ ] 3.1 Build bottom-sheet policy editor.
- [ ] 3.2 Add trust default template helpers.
- [ ] 3.3 Wire save/block/unblock/unpair actions.
- [ ] 3.4 Refresh device list/cache after policy updates.

## 4. Receive UX

- [ ] 4.1 Show policy context in manual offer dialog.
- [ ] 4.2 Handle auto-accepted offers in Activity without manual prompt.
- [ ] 4.3 Handle policy-rejected offers with clear copy.

## 5. Validation

- [ ] 5.1 Add Android Maestro flow for device detail and policy editor.
- [ ] 5.2 Run `pnpm typecheck`.
- [ ] 5.3 Run `openspec validate add-mobile-device-trust-policies --strict`.
