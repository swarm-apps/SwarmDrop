# send-progress Specification

## Purpose
TBD - created by archiving change fix-transfer-progress. Update Purpose after archive.
## Requirements
### Requirement: SendSession SHALL emit transfer-progress events

SendSession MUST integrate a ProgressTracker (wrapped in `Arc<std::sync::Mutex>`) and emit `transfer-progress` events during active file transfer. The ProgressTracker MUST be initialized in `SendSession::new()` with `total_bytes` calculated from the file list and `direction = "send"`.

#### Scenario: Progress emitted after each chunk response
- **WHEN** SendSession processes a ChunkRequest and successfully encrypts the chunk
- **THEN** it MUST call `progress.add_bytes(plaintext_len)` and `progress.emit_progress(app)` before returning the Chunk response

#### Scenario: Progress event contains valid speed and ETA
- **WHEN** the ProgressTracker has accumulated at least 2 samples within the 3-second sliding window
- **THEN** the emitted `transfer-progress` event MUST contain non-zero `speed` (bytes/sec) and a valid `eta` (seconds remaining)

#### Scenario: Progress is throttled at 200ms intervals
- **WHEN** multiple ChunkRequests are processed within 200ms
- **THEN** at most one `transfer-progress` event SHALL be emitted per 200ms interval (existing throttle behavior)

### Requirement: SendSession SHALL track per-file completion

SendSession MUST track the number of completed chunks per file using atomic counters. A file MUST be marked as "completed" when all its chunks have been served, regardless of the order in which chunks are requested.

#### Scenario: File marked complete when all chunks served
- **WHEN** the final chunk of a file is served (detected by per-file `chunks_done == total_chunks`)
- **THEN** the file's status in the progress tracker MUST transition from "transferring" to "completed" and `completed_files` counter MUST increment

#### Scenario: Concurrent chunk requests for same file
- **WHEN** multiple concurrent ChunkRequests arrive for different chunks of the same file
- **THEN** each completed chunk MUST atomically increment the per-file counter without race conditions

