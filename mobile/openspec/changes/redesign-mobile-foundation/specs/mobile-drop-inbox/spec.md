## ADDED Requirements

### Requirement: Mobile inbox native API

The mobile native bridge SHALL expose APIs to list inbox items, load inbox item detail, archive or unarchive an item, delete an item record, mark missing files, and repair missing inbox records for completed receives.

#### Scenario: List inbox items
- **WHEN** RN calls the inbox list API without archived items
- **THEN** native SHALL return non-deleted and non-archived inbox item summaries sorted by received time descending.

#### Scenario: Load inbox detail
- **WHEN** RN requests an existing inbox item detail
- **THEN** native SHALL return the item summary, file entries, and associated transfer projection when available.

### Requirement: Inbox content surface

The Inbox tab SHALL show successfully received content as content records, not transfer process records. Each item SHALL show title, source name, item count, total size, received time, content kind, and missing state.

#### Scenario: Completed receive creates visible content
- **WHEN** a receive transfer completes and an inbox item exists
- **THEN** the Inbox tab SHALL show the content item independent of Activity.

#### Scenario: Incomplete receive
- **WHEN** a receive transfer is paused, interrupted, failed, rejected, or cancelled
- **THEN** Inbox SHALL NOT show a content item for that incomplete transfer.

### Requirement: Inbox detail actions

Inbox detail SHALL provide mobile-appropriate actions for received files: open or share file, copy path or URI, archive or unarchive record, and delete record. Delete-local-file behavior SHALL require explicit destructive confirmation when supported.

#### Scenario: User opens received file
- **WHEN** the user taps an available received file in Inbox detail
- **THEN** the app SHALL open or share it through the platform file/share mechanism.

#### Scenario: User deletes record only
- **WHEN** the user deletes an inbox record without choosing local file deletion
- **THEN** the inbox record SHALL disappear while the local file remains untouched.

### Requirement: Missing file handling

Inbox SHALL preserve records for files that were moved or deleted outside SwarmDrop and SHALL mark them as missing instead of failing silently.

#### Scenario: Missing file open
- **WHEN** the user tries to open an inbox file that no longer exists
- **THEN** the app SHALL mark the file missing and show a recoverable user message.

### Requirement: Inbox and Activity separation

Inbox SHALL answer "what did I receive?" and Activity SHALL answer "what happened to the transfer?". The same completed receive MAY link between both surfaces but SHALL NOT be rendered as two equal primary records in the same surface.

#### Scenario: User views received content
- **WHEN** the user opens Inbox item detail
- **THEN** the screen SHALL focus on content operations and MAY link to transfer diagnostics as secondary context.

### Requirement: Android Maestro inbox validation

Inbox list, empty state, detail sheet/screen, archive control, and delete confirmation SHALL expose stable Android test IDs for Maestro validation.

#### Scenario: Maestro verifies empty Inbox
- **WHEN** an Android Maestro flow opens Inbox with no items
- **THEN** the Inbox empty state SHALL be visible by test ID.
