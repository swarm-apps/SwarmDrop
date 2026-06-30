# per-file-progress Specification

## Purpose
TBD - created by archiving change fix-transfer-progress. Update Purpose after archive.
## Requirements
### Requirement: TransferProgressEvent SHALL include per-file progress

`TransferProgressEvent` MUST contain a `files` field of type `Vec<FileProgressInfo>`, where each entry represents a file in the transfer session with its individual status and progress.

```rust
pub struct FileProgressInfo {
    pub file_id: u32,
    pub name: String,
    pub size: u64,
    pub transferred: u64,
    pub status: &'static str, // "pending" | "transferring" | "completed"
}
```

#### Scenario: Initial progress event shows all files as pending
- **WHEN** the first `transfer-progress` event is emitted for a new session
- **THEN** the `files` array MUST contain all files with `status = "pending"` and `transferred = 0`

#### Scenario: Active file shows transferring status with byte progress
- **WHEN** a file is currently being transferred (chunks are being pulled/served)
- **THEN** the file's status MUST be "transferring" and `transferred` MUST reflect the cumulative bytes of completed chunks for that file

#### Scenario: Completed file shows full progress
- **WHEN** all chunks of a file have been successfully transferred
- **THEN** the file's status MUST be "completed" and `transferred` MUST equal `size`

### Requirement: ProgressTracker SHALL maintain per-file state

ProgressTracker MUST internally track per-file progress with chunk-level granularity. It MUST support concurrent updates from multiple chunk tasks for both send and receive directions.

#### Scenario: Receive direction per-file tracking
- **WHEN** ReceiveSession processes files sequentially with concurrent chunks
- **THEN** each file's `FileProgressInfo` MUST accurately reflect its current chunk progress, and the `files` array MUST be included in every emitted `transfer-progress` event

#### Scenario: Send direction per-file tracking
- **WHEN** SendSession serves chunks from multiple files (potentially interleaved by the receiver)
- **THEN** each file's `FileProgressInfo` MUST accurately reflect the number of chunks served, with file status transitioning from "pending" → "transferring" → "completed"

### Requirement: Frontend types SHALL match backend event structure

The TypeScript `TransferProgressEvent` interface in `src/commands/transfer.ts` MUST include a `files` field matching the Rust `FileProgressInfo` serialization.

#### Scenario: TypeScript type includes files array
- **WHEN** a `transfer-progress` event is received by the frontend
- **THEN** `event.payload.files` MUST be an array of `{ fileId: number, name: string, size: number, transferred: number, status: "pending" | "transferring" | "completed" }`

### Requirement: Transfer detail page SHALL display per-file status

The transfer detail page (`$sessionId.lazy.tsx`) MUST render each file's individual status and progress from the `files` array in `TransferProgressEvent`.

#### Scenario: File list shows mixed statuses during transfer
- **WHEN** a multi-file transfer is in progress with some files completed and one actively transferring
- **THEN** the UI MUST display completed files with a completed indicator, the active file with a progress bar or percentage, and pending files with a pending indicator

