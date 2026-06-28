## ADDED Requirements

### Requirement: Projection-first transfer store

The RN transfer store SHALL keep transfer projections as the single source of transfer state and SHALL NOT keep a separate `dbHistory` model or old five-state history cache for new UI consumers.

#### Scenario: Progress event arrives
- **WHEN** a progress event arrives for an existing projection
- **THEN** the store SHALL keep the projection and progress in sync without creating a separate active session model.

#### Scenario: Completed transfer refreshes
- **WHEN** a transfer completes
- **THEN** Activity SHALL update from projection data rather than moving an item from an active list into a separate history list.

### Requirement: Activity grouping

The Activity surface SHALL group transfer projections into Active, Recoverable, Needs Attention, and Completed Diagnostics sections derived only from projection phase/reason/recoverable fields.

#### Scenario: Recoverable interrupted transfer
- **WHEN** a projection has `phase=suspended` and `recoverable=true`
- **THEN** Activity SHALL place it in the Recoverable section with a resume action.

#### Scenario: Fatal terminal transfer
- **WHEN** a projection has `phase=terminal` and `terminalReason=fatal_error`
- **THEN** Activity SHALL place it in Needs Attention and show the error reason.

### Requirement: Projection status language

The UI SHALL map transfer phase and reason into user-facing labels, including waiting confirmation, transferring, local paused, remote paused, interrupted, peer offline, app restarted, completed, cancelled, rejected, and fatal error.

#### Scenario: Remote paused projection
- **WHEN** a projection has `phase=suspended` and `suspendedReason=remote_paused`
- **THEN** the UI SHALL show that the other device paused the transfer.

#### Scenario: Rejected projection
- **WHEN** a projection has `phase=terminal` and `terminalReason=rejected`
- **THEN** the UI SHALL distinguish rejection from user cancellation.

### Requirement: Activity clearing is process scoped

Activity clear/delete entry points SHALL be named and placed as transfer process actions, not received-content actions. The native semantics are implemented by the follow-up content changes.

#### Scenario: User views completed receive diagnostics
- **WHEN** the user views a completed receive in Activity
- **THEN** the screen SHALL present it as transfer diagnostics and point users to Inbox for content actions.
