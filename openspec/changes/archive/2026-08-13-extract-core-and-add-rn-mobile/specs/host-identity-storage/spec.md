## ADDED Requirements

### Requirement: Host-provided identity storage
Core SHALL access persistent device identity through a host-provided keychain/secure-store abstraction.

#### Scenario: Desktop identity storage
- **WHEN** desktop host constructs core
- **THEN** it provides a keychain adapter backed by OS keychain

#### Scenario: RN identity storage
- **WHEN** RN host constructs mobile core
- **THEN** it provides a secure storage adapter backed by Expo SecureStore or an equivalent native secure store

### Requirement: Passwordless default identity initialization
系统 SHALL initialize or load device identity without requiring an application password in the default startup path.

#### Scenario: Fresh install starts
- **WHEN** a fresh install starts
- **THEN** host identity storage creates a stable keypair
- **AND** onboarding can proceed without asking the user to create a vault password

### Requirement: Legacy Stronghold data discarded
系统 SHALL treat the existing Stronghold-based identity and paired-device storage as deprecated desktop data that is not migrated.

#### Scenario: Legacy desktop data exists
- **WHEN** desktop host detects legacy Stronghold setup and no new keychain identity
- **THEN** it creates a new host keychain identity
- **AND** it does not require the old vault password

### Requirement: Stable PeerId across hosts
系统 SHALL preserve the user's PeerId across restarts after host identity storage has been initialized.

#### Scenario: Keychain identity is reused
- **WHEN** host keychain identity already exists
- **THEN** the PeerId derived on the next startup equals the previous keychain PeerId
