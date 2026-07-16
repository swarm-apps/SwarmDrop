## ADDED Requirements

### Requirement: Shared core dependency sync

The mobile-core crate SHALL depend on a SwarmDrop shared core revision that includes transfer coordinator projections, data-channel transfer support, inbox database APIs, trusted device policies, and network discovery config.

#### Scenario: Build against target core
- **WHEN** `mobile-core` is built
- **THEN** it SHALL compile against the selected shared SwarmDrop revision without requiring old history API compatibility.

### Requirement: Runtime command alignment

Mobile transfer commands SHALL call shared core coordinator/runtime APIs for accept, reject, pause, cancel, resume, and projection retrieval.

#### Scenario: Resume command
- **WHEN** RN requests resume for a recoverable session
- **THEN** native SHALL route the request through the shared core resume coordination path.
