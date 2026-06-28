## ADDED Requirements

### Requirement: Projection-first native transfer API

The mobile native bridge SHALL expose transfer projection records that include session ID, direction, peer identity, phase, suspended reason, terminal reason, recoverable flag, epoch, bytes, timestamps, error message, policy action, policy reason, save location, and file projections.

#### Scenario: Load projections
- **WHEN** RN calls the mobile transfer projection list API
- **THEN** native SHALL return all known transfer projections sorted with newest activity first.

#### Scenario: Projection update event
- **WHEN** shared core publishes a transfer projection update
- **THEN** RN SHALL receive the corresponding projection event and update the store entry for that session ID.

### Requirement: Transfer store single source of truth

The RN transfer store SHALL keep projections as the single source of transfer state and SHALL NOT keep a separate `dbHistory` model or old five-state history cache.

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

### Requirement: Activity clearing does not delete inbox content

Activity clear/delete operations SHALL operate only on transfer process records and SHALL NOT delete Drop Inbox records or local received files.

#### Scenario: User clears activity
- **WHEN** the user confirms clearing Activity records
- **THEN** Inbox items and received local files SHALL remain available.

### Requirement: Android Maestro activity validation

Activity SHALL expose stable Android test IDs for active, recoverable, attention, and completed diagnostic sections.

#### Scenario: Maestro verifies empty Activity
- **WHEN** an Android Maestro flow opens Activity with no projections
- **THEN** the empty Activity state SHALL be visible by test ID.
