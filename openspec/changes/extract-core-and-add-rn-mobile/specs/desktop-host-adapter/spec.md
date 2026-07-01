## ADDED Requirements

### Requirement: Tauri host wraps core
Tauri desktop host SHALL call `swarmdrop-core` for identity, network, pairing, transfer and device operations, while keeping Tauri-specific IPC and plugin behavior in `src-tauri`.

#### Scenario: Frontend calls network start
- **WHEN** frontend invokes the desktop start command
- **THEN** the Tauri command delegates network startup to the core runtime
- **AND** the command does not duplicate core network startup logic

### Requirement: Desktop event forwarding
Tauri desktop host SHALL forward core events to existing frontend event names where possible.

#### Scenario: Transfer progress event
- **WHEN** core emits transfer progress
- **THEN** desktop host forwards the event to the Tauri frontend using the current transfer progress event contract or a documented compatible replacement

### Requirement: Desktop host adapters
Tauri desktop host SHALL provide implementations for required core host traits.

#### Scenario: Core requests file read
- **WHEN** core needs to read a selected desktop file source
- **THEN** desktop host file access adapter reads from the local path or existing Android Tauri file source implementation

#### Scenario: Core requests secure identity bytes
- **WHEN** core requests device identity keypair bytes
- **THEN** desktop host keychain adapter loads or creates them using OS keychain or the configured legacy migration path

### Requirement: Preserve desktop behavior
The refactored desktop app SHALL preserve existing user-facing pairing, transfer, device list, transfer history and settings behavior unless explicitly changed by this change.

#### Scenario: Existing desktop transfer flow
- **WHEN** a desktop user selects files, sends them to a paired online device, and the peer accepts
- **THEN** the transfer completes with progress, completion events and history records comparable to the pre-refactor behavior
