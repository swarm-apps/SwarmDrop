## ADDED Requirements

### Requirement: Inbox tab foundation

The mobile shell SHALL include Inbox as a primary tab for received content. In this foundation change, Inbox SHALL provide empty/loading/list shell states and a stable store boundary, while native Inbox APIs and content actions are implemented by `add-mobile-drop-inbox`.

#### Scenario: Empty Inbox
- **WHEN** no Inbox records are loaded
- **THEN** the Inbox tab SHALL show a received-content empty state independent of Activity.

#### Scenario: Inbox loading boundary
- **WHEN** Inbox data is refreshing
- **THEN** the Inbox tab SHALL show a loading state without blocking Devices or Activity navigation.

### Requirement: Inbox and Activity separation copy

Inbox SHALL answer "what did I receive?" and Activity SHALL answer "what happened to the transfer?". Foundation copy and empty states SHALL communicate that distinction.

#### Scenario: User opens empty Inbox
- **WHEN** the user opens Inbox before any completed receives
- **THEN** the app SHALL describe Inbox as the place where received content will appear.

#### Scenario: User opens Activity
- **WHEN** the user opens Activity
- **THEN** completed diagnostics SHALL NOT be described as received content actions.

### Requirement: Inbox extension points

Inbox row, detail, and action components SHALL be structured so the follow-up Inbox change can attach summary/detail records, archive/delete actions, and missing-file states without another shell rewrite.

#### Scenario: Follow-up adds native Inbox records
- **WHEN** `add-mobile-drop-inbox` binds native item summaries
- **THEN** it SHALL be able to populate the existing Inbox tab instead of introducing a new primary route.
