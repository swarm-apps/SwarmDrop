## 1. Native API

- [ ] 1.1 Add mobile bridge records for inbox summary, detail, and file entry.
- [ ] 1.2 Expose list, detail, archive, delete, mark-missing, and repair commands.
- [ ] 1.3 Add event or refresh hooks for receive completion and inbox repair.

## 2. Store

- [ ] 2.1 Add inbox store for items, selected detail, loading, archived filter, and action state.
- [ ] 2.2 Implement list/detail refresh and action helpers.
- [ ] 2.3 Ensure Activity clear actions do not touch inbox state.

## 3. UI

- [ ] 3.1 Build Inbox tab empty/list states.
- [ ] 3.2 Build Inbox detail screen or sheet.
- [ ] 3.3 Implement open/share/copy/archive/delete actions.
- [ ] 3.4 Implement missing-file presentation.

## 4. Validation

- [ ] 4.1 Add Android Maestro flow for Inbox empty/list/detail.
- [ ] 4.2 Add Android Maestro flow for delete confirmation.
- [ ] 4.3 Run `pnpm typecheck`.
- [ ] 4.4 Run `openspec validate add-mobile-drop-inbox --strict`.
