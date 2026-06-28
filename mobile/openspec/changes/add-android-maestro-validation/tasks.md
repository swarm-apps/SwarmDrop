## 1. Test IDs

- [x] 1.1 Add stable test IDs to primary tabs and top-level screens.
- [x] 1.2 Add test IDs to empty states for Devices, Inbox, and Activity.
- [x] 1.3 Add test IDs to device detail, policy editor, network settings, inbox detail, activity sections, destructive confirmations, and receive-offer dialog controls.

## 2. Maestro Flows

- [x] 2.1 Add Android shell navigation flow.
- [x] 2.2 Add Android empty-state flow.
- [x] 2.3 Add Android device trust editor flow.
- [x] 2.4 Add Android network discovery settings flow.
- [x] 2.5 Add Android inbox detail/delete flow when fixture state is available.
- [x] 2.6 Add Android receive offer presentation flow when fixture/mock state is available. No safe fixture exists yet; stable receive-offer dialog test IDs are present and docs keep the full flow out of required smoke until a fixture lands.

## 3. Documentation

- [x] 3.1 Document local Android Maestro setup and run command.
- [x] 3.2 Document artifact/log/screenshot locations.
- [x] 3.3 Document that iOS is out of scope for this validation change.

## 4. Verification

- [x] 4.1 Run Android Maestro suite locally.
- [x] 4.2 Run `pnpm typecheck`.
- [x] 4.3 Run `openspec validate add-android-maestro-validation --strict`.
