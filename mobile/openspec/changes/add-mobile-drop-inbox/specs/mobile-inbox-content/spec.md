## ADDED Requirements

### Requirement: Inbox list

The mobile app SHALL show successfully received inbox items in a dedicated Inbox surface with title, source, count, total size, received time, content kind, and missing state.

#### Scenario: Completed receive appears
- **WHEN** native returns an inbox item for a completed receive
- **THEN** Inbox SHALL render the item in newest-first order.

### Requirement: Inbox detail

The mobile app SHALL show inbox item detail with file entries and linked transfer diagnostics when native provides them.

#### Scenario: User opens inbox item
- **WHEN** the user taps an inbox item
- **THEN** the app SHALL show its file list and source metadata.

### Requirement: No incomplete transfers

Inbox SHALL NOT show paused, failed, cancelled, rejected, or interrupted transfers unless native has created a completed inbox item.

#### Scenario: Interrupted receive
- **WHEN** a receive transfer is interrupted before completion
- **THEN** the item SHALL remain in Activity and SHALL NOT appear as Inbox content.
