## ADDED Requirements

### Requirement: Trust badge foundation

Device cards, rows, and details SHALL have a trust badge slot for owned, collaborator, temporary, and blocked devices. Until persisted trust policy data is implemented, unknown or legacy devices SHALL default visually and behaviorally to collaborator.

#### Scenario: Known trust level
- **WHEN** a device record contains a trust level
- **THEN** the device surface SHALL show the matching trust badge.

#### Scenario: Unknown trust level
- **WHEN** a device record has no trust level
- **THEN** the device surface SHALL show collaborator semantics rather than hiding trust context.

### Requirement: Device detail foundation

Tapping a paired device SHALL open a device detail surface with identity, connection status, transfer shortcut, trust summary slot, policy summary slot, and management action slots.

#### Scenario: User opens paired device
- **WHEN** the user taps a paired device
- **THEN** the app SHALL show device detail instead of immediately forcing file selection.

#### Scenario: User sends from device detail
- **WHEN** the user taps the send action on an online non-blocked paired device detail
- **THEN** the app SHALL open the send preparation flow for that peer.

### Requirement: Policy editor entry point

The foundation SHALL provide a mobile-appropriate policy editor entry point and bottom-sheet container. Persisted policy editing, default templates, and native save/block/unblock commands SHALL be implemented by `add-mobile-device-trust-policies`.

#### Scenario: User opens policy editor before native policy sync
- **WHEN** the user opens the policy editor entry point
- **THEN** the app SHALL show the policy surface or an unavailable state without breaking device detail navigation.

### Requirement: Trust extension points

Device detail and trust badge components SHALL be structured so the follow-up trust-policy change can attach real policy fields and destructive confirmations without replacing the Devices tab.

#### Scenario: Follow-up adds native policy fields
- **WHEN** `add-mobile-device-trust-policies` binds persisted trust data
- **THEN** it SHALL populate the existing trust badge, detail summary, and policy editor container.
