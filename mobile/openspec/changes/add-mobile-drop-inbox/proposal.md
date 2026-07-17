## Why

Desktop SwarmDrop now separates received content from transfer process logs through Drop Inbox. Mobile needs the same content-first receive experience after the bridge can expose inbox APIs.

## What Changes

- Add a mobile Inbox surface for successfully received files/directories.
- Add inbox list and detail UI, backed by native inbox summary/detail APIs.
- Add mobile actions for opening/sharing files, copying paths/URIs, archiving records, deleting records, and marking missing files.
- Keep Activity clearing separate from Inbox retention.
- Add Android Maestro coverage for Inbox empty, list, detail, and destructive confirmation flows.

## Capabilities

### New Capabilities

- `mobile-inbox-content`: Mobile listing and detail of successfully received inbox content.
- `mobile-inbox-file-actions`: Mobile file/content actions and deletion semantics for inbox items.

### Modified Capabilities

None. This is a mobile implementation of a desktop capability with no archived mobile spec.

## Impact

- `mobile-core`: inbox list/detail/archive/delete/missing/repair APIs.
- `src/stores`: new inbox store or equivalent domain state.
- `src/app` and `src/components`: Inbox tab, detail screen/sheet, item rows, file actions, confirmations.
- Android Maestro flows for Inbox.
