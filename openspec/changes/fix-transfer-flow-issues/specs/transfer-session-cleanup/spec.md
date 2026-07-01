## ADDED Requirements

### Requirement: Prepared transfer auto-expiration
TransferManager SHALL automatically remove `PreparedTransfer` entries from the `prepared` DashMap when they have not been used within 5 minutes of creation.

#### Scenario: Prepared transfer expires after timeout
- **WHEN** a `PreparedTransfer` is created via `prepare()` and no `send_offer()` is called within 5 minutes
- **THEN** the `PreparedTransfer` entry (including its `FileSource` handles) SHALL be removed from the `prepared` DashMap

#### Scenario: Prepared transfer used before timeout
- **WHEN** a `PreparedTransfer` is created via `prepare()` and `send_offer()` is called within 5 minutes
- **THEN** the `PreparedTransfer` entry SHALL be consumed normally and not affected by the cleanup timer

### Requirement: Send session idle timeout
TransferManager SHALL automatically remove `SendSession` entries from the `send_sessions` DashMap when no chunk request has been received for 30 minutes.

#### Scenario: Send session idle timeout
- **WHEN** a `SendSession` exists and no `ChunkRequest` is received for 30 minutes
- **THEN** the `SendSession` SHALL be cancelled and removed from `send_sessions`, and the corresponding DB record SHALL be marked as failed

#### Scenario: Active send session not cleaned up
- **WHEN** a `SendSession` receives chunk requests within the 30-minute window
- **THEN** the idle timer SHALL be reset and the session SHALL continue normally

### Requirement: Pending offer auto-expiration
TransferManager SHALL automatically remove `PendingOffer` entries from the `pending` DashMap when they have not been accepted or rejected within 5 minutes.

#### Scenario: Pending offer expires
- **WHEN** a `PendingOffer` is cached via `cache_inbound_offer()` and neither `accept_and_start_receive()` nor `reject_and_respond()` is called within 5 minutes
- **THEN** the `PendingOffer` SHALL be removed from the `pending` DashMap and the frontend `pendingOffers` queue SHALL be updated accordingly

### Requirement: Cleanup task lifecycle
The cleanup task SHALL be spawned as a tokio task during `TransferManager` initialization and SHALL be gracefully stopped when the P2P node shuts down.

#### Scenario: Cleanup task starts with TransferManager
- **WHEN** `TransferManager::new()` is called
- **THEN** a background tokio task SHALL be spawned that runs a cleanup sweep every 60 seconds

#### Scenario: Cleanup task stops on shutdown
- **WHEN** the P2P node shuts down (via `CancellationToken` cancellation)
- **THEN** the cleanup task SHALL exit gracefully without blocking shutdown

### Requirement: Resume transfer returns session metadata
The `resume_transfer` Rust command SHALL return complete session metadata so the frontend can create a runtime session for progress tracking.

#### Scenario: Successful resume creates runtime session
- **WHEN** `resume_transfer` is called and the remote peer accepts the resume request
- **THEN** the command SHALL return session metadata (session_id, direction, peer_id, peer_name, files with sizes, total_size) and the frontend SHALL create a runtime `TransferSession` in the active sessions map

#### Scenario: Resume transfer rejected
- **WHEN** `resume_transfer` is called and the remote peer rejects the resume request
- **THEN** the command SHALL return an error with the rejection reason

### Requirement: Cancel event bitmap safety
When processing an incoming Cancel event for a receive session, the system SHALL wait for bitmap flush to complete before cleaning up .part files.

#### Scenario: Cancel event with active bitmap flush
- **WHEN** a Cancel event is received for an active receive session that has in-progress bitmap writes
- **THEN** the system SHALL call `cancel_and_wait()` (not just `cancel()`) before proceeding to `cleanup_part_files()`

### Requirement: Decrypt failure retry
When a chunk decryption fails during receive, the system SHALL retry the chunk request instead of immediately failing the entire file transfer.

#### Scenario: Decrypt failure triggers retry
- **WHEN** `decrypt_chunk()` returns an error for a received chunk
- **THEN** the system SHALL retry the chunk request up to 3 times (same as network error retry), with exponential backoff

#### Scenario: Decrypt failure exhausts retries
- **WHEN** decryption fails 3 consecutive times for the same chunk
- **THEN** the file transfer SHALL fail with a descriptive error message

### Requirement: Unified offer consumption
The `TransferOfferDialog` component SHALL be the sole consumer of the `pendingOffers` queue. No other component SHALL call `shiftOffer()`.

#### Scenario: Offer received while on any page
- **WHEN** a transfer offer arrives and the user is on any page (including `/receive`)
- **THEN** the `TransferOfferDialog` SHALL display the offer as a modal dialog

#### Scenario: Receive page does not consume offers
- **WHEN** the user navigates to `/receive`
- **THEN** the page SHALL display a "waiting for incoming transfers" message without consuming from `pendingOffers`

### Requirement: Send waiting state with cancel ability
The send flow SHALL provide a visible "waiting for confirmation" state and allow the user to cancel during the wait.

#### Scenario: Send enters waiting state
- **WHEN** `startSend` is called and the offer is sent to the remote peer
- **THEN** a runtime session with `waiting_accept` status SHALL be created immediately and the UI SHALL display "waiting for peer confirmation"

#### Scenario: User cancels during waiting
- **WHEN** the user clicks cancel while in `waiting_accept` state
- **THEN** the pending send SHALL be cancelled and the session SHALL be removed

#### Scenario: Peer accepts during waiting
- **WHEN** the remote peer accepts the offer
- **THEN** the session status SHALL transition to `transferring` and progress updates SHALL begin

### Requirement: Cancel optimistic update
When the user initiates a cancel action, the frontend SHALL immediately update the UI state before waiting for Rust backend confirmation.

#### Scenario: User cancels active transfer
- **WHEN** the user clicks the cancel button on an active transfer
- **THEN** the session SHALL be immediately moved to history with `cancelled` status, and the Rust cancel command SHALL be called asynchronously

#### Scenario: Rust cancel command fails after optimistic update
- **WHEN** the optimistic cancel UI update succeeds but the Rust cancel command fails
- **THEN** a toast error message SHALL be displayed to the user

### Requirement: Simplified history data source
The frontend transfer store SHALL use the database as the single source of truth for transfer history, removing the in-memory `history` array.

#### Scenario: Transfer completes
- **WHEN** a `transfer-complete` event is received
- **THEN** the session SHALL be removed from the active `sessions` map and `loadHistory()` SHALL be called (with 500ms delay to ensure DB write completion) to refresh the history from the database

#### Scenario: History page loads
- **WHEN** the user navigates to the transfer history page
- **THEN** history data SHALL be loaded exclusively from the database via `getTransferHistory()`

### Requirement: TransferItem render optimization
Each `TransferItem` component SHALL subscribe to its own session data independently to prevent unnecessary re-renders of sibling items.

#### Scenario: Single session progress update
- **WHEN** one active session receives a progress update
- **THEN** only the `TransferItem` for that session SHALL re-render, not other items in the list
