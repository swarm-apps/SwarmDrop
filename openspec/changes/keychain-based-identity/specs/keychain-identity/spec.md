## ADDED Requirements

### Requirement: Platform keychain identity storage
The system SHALL persist the device's libp2p Ed25519 keypair in a platform-backed secure store and SHALL load the same keypair across app restarts.

#### Scenario: First launch creates identity
- **WHEN** the app starts and no keypair exists in the platform keychain
- **THEN** the backend generates a new Ed25519 keypair, stores its libp2p protobuf encoding in the platform keychain, and exposes the derived PeerId as the device identity

#### Scenario: Restart reuses identity
- **WHEN** the app starts and a keypair exists in the platform keychain
- **THEN** the backend loads that keypair and exposes the same PeerId as previous launches

### Requirement: Backend-owned keypair access
The system SHALL keep keypair loading, decoding, and libp2p registration in the backend runtime instead of requiring the frontend to provide raw keypair bytes.

#### Scenario: Network starts without frontend keypair
- **WHEN** the frontend requests network startup after app initialization
- **THEN** the backend starts the libp2p node using the keypair loaded from the platform keychain
- **AND** the frontend does not pass raw keypair bytes to the start command

### Requirement: Legacy Stronghold identity migration
The system SHALL support one-time migration from the legacy Stronghold-backed secret store to platform keychain identity storage.

#### Scenario: Legacy vault detected and keychain empty
- **WHEN** the app detects legacy setup state and no keypair exists in the platform keychain
- **THEN** the app prompts the user to unlock the legacy vault for migration

#### Scenario: Migration succeeds
- **WHEN** the user unlocks the legacy vault and the vault contains a keypair
- **THEN** the system saves the keypair to the platform keychain
- **AND** verifies that the backend can load the same PeerId from the platform keychain
- **AND** marks identity migration complete

#### Scenario: Migration fails
- **WHEN** keychain write, keypair decode, or verification fails during migration
- **THEN** the system leaves the legacy Stronghold vault unchanged
- **AND** does not mark identity migration complete
- **AND** shows a recoverable error

### Requirement: Keychain unavailable handling
The system SHALL clearly handle platform keychain unavailability without silently losing the user's stable identity.

#### Scenario: Keychain unavailable with no existing identity
- **WHEN** the backend cannot access the platform keychain and no migrated identity is available
- **THEN** the system reports that persistent device identity is unavailable
- **AND** does not silently mark onboarding or migration complete with a non-persistent keypair
