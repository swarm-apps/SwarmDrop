## ADDED Requirements

### Requirement: Trust data on devices

Mobile paired device records SHALL include trust level, receive policy, and trust-confirmed state when available.

#### Scenario: Offline paired device
- **WHEN** the node is stopped
- **THEN** paired device cache SHALL still expose trust and policy fields.

### Requirement: Trust badge display

Device surfaces SHALL display badges for owned, collaborator, temporary, and blocked trust levels.

#### Scenario: Blocked device
- **WHEN** a device is blocked
- **THEN** the app SHALL show a blocked badge and hide or disable send actions.

### Requirement: Policy editor

The app SHALL allow editing trust level and receive policy fields for paired devices.

#### Scenario: Save policy
- **WHEN** the user saves a receive policy
- **THEN** native SHALL persist it and the device surface SHALL refresh.
