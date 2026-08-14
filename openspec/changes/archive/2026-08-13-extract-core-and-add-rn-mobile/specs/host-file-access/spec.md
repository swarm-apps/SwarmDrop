## ADDED Requirements

### Requirement: Host-provided file source access
Core SHALL read selected send files through host-provided file source abstractions rather than directly depending on Tauri or React Native file APIs.

#### Scenario: Desktop path source
- **WHEN** desktop host provides a local file path source
- **THEN** core can enumerate metadata, compute hash, and read chunks through the file access adapter

#### Scenario: RN picked file source
- **WHEN** RN host provides a picked file copied to cache or a native file reference
- **THEN** core can enumerate metadata, compute hash, and read chunks through the mobile file access adapter

### Requirement: Host-provided file sink access
Core SHALL write received files through host-provided sink abstractions.

#### Scenario: Desktop receive sink
- **WHEN** desktop host accepts a receive offer with a local save path
- **THEN** core writes chunks through the desktop sink and finalizes the file

#### Scenario: RN receive sink
- **WHEN** RN host accepts a receive offer
- **THEN** core writes chunks through the RN mobile sink into the selected MVP save location

### Requirement: Mobile MVP file access constraints
RN mobile MVP SHALL document and enforce first-milestone file access limitations.

#### Scenario: RN MVP sends files
- **WHEN** a user selects files in RN MVP
- **THEN** the app may copy selected files into app cache/private storage before sending
- **AND** the UI communicates any unsupported directory or public-storage behavior gracefully

### Requirement: Transfer resume compatibility
File access adapters SHALL provide enough stable source/sink metadata for transfer history and resume decisions supported by the host.

#### Scenario: Resume unsupported for a mobile source
- **WHEN** a mobile file source cannot provide stable access for resume after app restart
- **THEN** the system marks resume as unavailable for that session rather than pretending resume is supported
