# chunk-transfer Specification

## Purpose
TBD - created by archiving change chunk-transfer. Update Purpose after archive.
## Requirements
### Requirement: Protocol extension for chunk transfer
The system SHALL extend `TransferRequest` and `TransferResponse` enums with `ChunkRequest`, `Chunk`, `Complete`, `Cancel`, `Ack` variants to support chunk-based data transfer over the existing libp2p request-response channel.

#### Scenario: ChunkRequest and Chunk round-trip
- **WHEN** the receiver sends a `TransferRequest::ChunkRequest { session_id, file_id, chunk_index }` to the sender
- **THEN** the sender SHALL respond with `TransferResponse::Chunk { session_id, file_id, chunk_index, data, is_last }` containing the encrypted chunk data

#### Scenario: Complete and Ack round-trip
- **WHEN** the receiver sends `TransferRequest::Complete { session_id }` after all files are received and verified
- **THEN** the sender SHALL respond with `TransferResponse::Ack { session_id }` and clean up the send session

#### Scenario: Cancel notification
- **WHEN** either party sends `TransferRequest::Cancel { session_id, reason }`
- **THEN** the receiving party SHALL clean up the session and NOT send further requests for that session

### Requirement: Sender responds to ChunkRequest with encrypted data
The sender SHALL read the requested file chunk from disk (256KB per chunk), encrypt it using the session's TransferCrypto (XChaCha20-Poly1305 with deterministic nonce), and return the ciphertext in a Chunk response. File reading SHALL use `tokio::task::spawn_blocking` to avoid blocking the async runtime.

#### Scenario: Normal chunk read and encrypt
- **WHEN** the sender receives a ChunkRequest for a valid session_id, file_id, and chunk_index
- **THEN** the sender SHALL read 256KB (or remaining bytes for the last chunk) from the file at the correct offset, encrypt it, and respond with a Chunk containing the ciphertext and `is_last = true` if it is the final chunk

#### Scenario: Invalid session or file
- **WHEN** the sender receives a ChunkRequest with an unknown session_id or file_id
- **THEN** the sender SHALL respond with an error response (Protocol error)

### Requirement: Receiver pulls chunks with concurrency control
The receiver SHALL spawn a background tokio task upon accepting an offer. This task SHALL use a `tokio::sync::Semaphore` with 8 permits to control concurrent ChunkRequest sends. For each file, the receiver SHALL request chunks sequentially by index (0, 1, 2, ...) up to `total_chunks`, with up to 8 requests in flight simultaneously.

#### Scenario: 8-concurrent chunk pulling
- **WHEN** the receiver starts pulling a file with 100 chunks
- **THEN** the receiver SHALL have at most 8 ChunkRequest calls in flight at any time

#### Scenario: Multi-file sequential processing
- **WHEN** the transfer contains multiple files
- **THEN** the receiver SHALL process files one by one in order, completing all chunks of one file before starting the next

### Requirement: Receiver decrypts and writes chunks to disk
The receiver SHALL decrypt each received chunk using TransferCrypto, then write the plaintext to a `.part` temporary file at the correct offset. Writing SHALL use `tokio::task::spawn_blocking`.

#### Scenario: Chunk write at correct offset
- **WHEN** the receiver receives chunk_index=5 for a file
- **THEN** the receiver SHALL write the decrypted data at byte offset `5 * 256 * 1024` in the `.part` file

#### Scenario: Decryption failure triggers retry
- **WHEN** a chunk fails to decrypt
- **THEN** the receiver SHALL retry the ChunkRequest up to 3 times with exponential backoff (500ms, 1s, 2s) before marking the transfer as failed

### Requirement: File verification with BLAKE3 checksum
After all chunks of a file are received and written, the receiver SHALL compute the BLAKE3 checksum of the assembled file and compare it against the checksum provided in the original Offer. The verification SHALL use `tokio::task::spawn_blocking`.

#### Scenario: Checksum matches
- **WHEN** the computed BLAKE3 checksum matches the expected checksum
- **THEN** the receiver SHALL rename the `.part` file to its final name (preserving relative_path directory structure)

#### Scenario: Checksum mismatch
- **WHEN** the computed BLAKE3 checksum does NOT match
- **THEN** the receiver SHALL mark the transfer as failed, delete the `.part` file, and emit a `transfer-failed` event with an error message indicating checksum mismatch

### Requirement: Progress tracking with speed and ETA
The system SHALL track transfer progress and emit `transfer-progress` events to the frontend via `app.emit`. Progress events SHALL be throttled to at most one every 200ms. Each progress event SHALL include: session_id, direction, current file info, total/transferred bytes, speed (bytes/sec), and estimated time remaining.

#### Scenario: Progress event throttling
- **WHEN** multiple chunks complete within a 200ms window
- **THEN** the system SHALL emit at most one `transfer-progress` event for that window, reflecting the latest state

#### Scenario: Speed calculation with sliding window
- **WHEN** calculating transfer speed
- **THEN** the system SHALL use a 3-second sliding window of (timestamp, cumulative_bytes) samples to compute average speed in bytes/sec

#### Scenario: ETA estimation
- **WHEN** speed > 0 and remaining bytes > 0
- **THEN** the system SHALL compute ETA as `remaining_bytes / speed` in seconds

### Requirement: Transfer completion flow
When all files are received and verified, the receiver SHALL send a `TransferRequest::Complete` to the sender and emit a `transfer-complete` event to the frontend. The sender SHALL respond with an Ack and emit its own `transfer-complete` event.

#### Scenario: Successful completion
- **WHEN** all files pass BLAKE3 verification
- **THEN** the receiver SHALL send Complete, the sender SHALL respond with Ack, and both sides SHALL emit `transfer-complete` events with session_id, direction, total_bytes, and elapsed time

#### Scenario: Receiver completion event includes save path
- **WHEN** the receiver completes a transfer
- **THEN** the `transfer-complete` event SHALL include the `save_path` so the frontend can offer "Open folder"

### Requirement: Bidirectional cancel
Either party SHALL be able to cancel an active transfer. Cancellation SHALL: (1) cancel all in-flight ChunkRequests via CancellationToken, (2) send a Cancel protocol message to the peer, (3) clean up local session state, (4) delete incomplete `.part` files on the receiver side, and (5) emit a `transfer-failed` event with reason "cancelled".

#### Scenario: Receiver cancels
- **WHEN** the receiver calls `cancel_receive(session_id)`
- **THEN** the receiver SHALL cancel ongoing chunk pulls, send a Cancel message to the sender, delete `.part` files, and emit `transfer-failed`

#### Scenario: Sender cancels
- **WHEN** the sender calls `cancel_send(session_id)`
- **THEN** the sender SHALL send a Cancel message to the receiver and clean up the send session

#### Scenario: Peer receives Cancel
- **WHEN** a party receives a Cancel message for an active session
- **THEN** it SHALL clean up the session and emit `transfer-failed` with reason indicating peer cancellation

### Requirement: Chunk-level retry with exponential backoff
The receiver SHALL retry failed ChunkRequests (timeout, network error, decryption failure) up to 3 times per chunk. Retry delays SHALL follow exponential backoff: 500ms, 1s, 2s (capped at 2s). After 3 failures on the same chunk, the entire transfer SHALL be marked as failed.

#### Scenario: Retry on timeout
- **WHEN** a ChunkRequest times out (no response within 30 seconds)
- **THEN** the receiver SHALL retry with a 500ms delay, then 1s, then 2s

#### Scenario: Max retries exceeded
- **WHEN** a chunk fails 3 times consecutively
- **THEN** the receiver SHALL cancel the entire transfer and emit `transfer-failed` with the error details

### Requirement: Save path and directory structure reconstruction
The receiver SHALL create the directory structure based on `relative_path` of each file under the user-specified `save_path`. The `accept_receive` command's `save_path` parameter SHALL be used as the root directory for file writing.

#### Scenario: Flat file save
- **WHEN** a file has `relative_path = "photo.jpg"` and `save_path = "/home/user/Downloads"`
- **THEN** the file SHALL be written to `/home/user/Downloads/photo.jpg`

#### Scenario: Nested directory reconstruction
- **WHEN** a file has `relative_path = "project/src/main.rs"` and `save_path = "/home/user/Downloads"`
- **THEN** the system SHALL create `/home/user/Downloads/project/src/` and write the file as `main.rs`

### Requirement: Register cancel commands in Tauri
The `cancel_send` and `cancel_receive` commands SHALL be registered in `lib.rs`'s `generate_handler!` macro. These commands SHALL be callable from the frontend.

#### Scenario: cancel_send registered and callable
- **WHEN** the frontend invokes `cancel_send` with a valid session_id
- **THEN** the Tauri IPC layer SHALL route the call to the `cancel_send` command handler

#### Scenario: cancel_receive registered and callable
- **WHEN** the frontend invokes `cancel_receive` with a valid session_id
- **THEN** the Tauri IPC layer SHALL route the call to the `cancel_receive` command handler

