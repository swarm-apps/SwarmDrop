## ADDED Requirements

### Requirement: Passwordless default startup
The system SHALL allow users to enter the main SwarmDrop application after onboarding without entering an application password by default.

#### Scenario: Onboarded user starts app
- **WHEN** an onboarded user launches the app
- **THEN** the root route initializes backend identity and app stores
- **AND** navigates to the main app without showing a password unlock page

### Requirement: Onboarding replaces mandatory password setup
The system SHALL use onboarding state to determine whether to show first-run setup screens, and SHALL NOT require users to create a password as part of default setup.

#### Scenario: New user starts app
- **WHEN** a user launches SwarmDrop for the first time
- **THEN** the app shows onboarding
- **AND** identity initialization can complete without collecting an application password

### Requirement: Optional app lock
The system SHALL support an optional app lock that gates UI access after app startup, independent of keypair storage.

#### Scenario: App lock disabled
- **WHEN** app lock is disabled
- **THEN** main app routes do not require an unlocked UI session

#### Scenario: App lock enabled
- **WHEN** app lock is enabled and the UI session is locked
- **THEN** protected app routes redirect to the app lock screen
- **AND** unlocking the app lock does not expose, copy, or decrypt the libp2p keypair in frontend state

### Requirement: Legacy migration unlock is temporary
The system SHALL show the legacy password/biometric unlock flow only when needed to migrate existing Stronghold data.

#### Scenario: Migration already complete
- **WHEN** identity migration is complete or no legacy Stronghold setup exists
- **THEN** the app does not show the legacy unlock screen during normal startup

#### Scenario: Migration pending
- **WHEN** legacy Stronghold setup exists and migration is pending
- **THEN** the app may show a migration unlock screen before completing onboarding or entering the main app
