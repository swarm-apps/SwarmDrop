## 1. Native API

- [x] 1.1 Add mobile bridge records for inbox summary, detail, and file entry.
- [x] 1.2 Expose list, detail, archive, delete, mark-missing, and repair commands.
- [x] 1.3 Add event or refresh hooks for receive completion and inbox repair.

## 2. Store

- [x] 2.1 Add inbox store for items, selected detail, loading, archived filter, and action state.
- [x] 2.2 Implement list/detail refresh and action helpers.
- [x] 2.3 Ensure Activity clear actions do not touch inbox state.

## 3. UI

- [x] 3.1 Build Inbox tab empty/list states.
- [x] 3.2 Build Inbox detail screen or sheet.
- [x] 3.3 Implement open/share/copy/archive/delete actions.
- [x] 3.4 Implement missing-file presentation.

## 4. Validation

- [x] 4.1 Add Android Maestro flow for Inbox empty/list/detail.
- [x] 4.2 Add Android Maestro flow for delete confirmation.
- [x] 4.3 Run `pnpm typecheck`.
- [x] 4.4 Run `openspec validate add-mobile-drop-inbox --strict`.
