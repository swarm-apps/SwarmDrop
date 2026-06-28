## ADDED Requirements

### Requirement: Mobile device trust data

The mobile native bridge SHALL expose paired device trust level, receive policy, and trust-confirmed state wherever paired devices are listed or loaded from persistent storage.

#### Scenario: Paired device loaded offline
- **WHEN** the node is stopped and RN loads paired devices from persistent storage
- **THEN** each paired device SHALL include trust and receive-policy fields when available.

#### Scenario: Online device projection
- **WHEN** RN lists devices while the node is running
- **THEN** paired devices SHALL include current trust and receive-policy values from core.

### Requirement: Trust badges on device surfaces

Device cards, rows, and details SHALL show a trust badge for owned, collaborator, temporary, and blocked devices. Unknown or legacy devices SHALL default visually and behaviorally to collaborator.

#### Scenario: Owned device card
- **WHEN** a paired device has `trustLevel=owned`
- **THEN** the device surface SHALL show the owned-device trust badge.

#### Scenario: Blocked device card
- **WHEN** a paired device has `trustLevel=blocked`
- **THEN** the device surface SHALL show the blocked badge and SHALL NOT offer a send action.

### Requirement: Device detail surface

Tapping a paired device SHALL open a device detail surface with identity, connection status, transfer shortcut, trust summary, policy summary, and management actions.

#### Scenario: User opens paired device
- **WHEN** the user taps a paired device
- **THEN** the app SHALL show device detail instead of immediately forcing file selection.

#### Scenario: User sends from device detail
- **WHEN** the user taps the send action on an online non-blocked paired device detail
- **THEN** the app SHALL open the send preparation flow for that peer.

### Requirement: Mobile policy editor

The app SHALL provide a bottom-sheet policy editor for paired devices. It SHALL allow selecting trust level and editing receive policy fields: auto accept, require confirmation, max transfer bytes, allow directories, allow relay auto accept, save behavior, default save location, allow MCP send to device, and expiration.

#### Scenario: Change trust level to owned
- **WHEN** the user selects owned in the policy editor
- **THEN** the editor SHALL apply owned defaults before saving unless the user overrides advanced fields.

#### Scenario: Save policy
- **WHEN** the user saves a valid trust level and receive policy
- **THEN** native SHALL update persistent paired device metadata and RN SHALL refresh the affected device.

### Requirement: Policy-gated receive presentation

Incoming offers SHALL present policy context to the user. Auto-accepted offers SHALL explain that the transfer was accepted by device policy, and policy-rejected offers SHALL surface a clear non-interactive reason.

#### Scenario: Auto accepted owned device
- **WHEN** an incoming offer is auto-accepted by policy
- **THEN** the UI SHALL create or update the corresponding Activity projection and show policy context without asking for manual confirmation.

#### Scenario: Policy rejected offer
- **WHEN** an incoming offer is rejected by policy
- **THEN** the UI SHALL show or record the rejection reason and SHALL NOT show an accept button.

### Requirement: Destructive trust actions

Blocking, unblocking, and unpairing SHALL require explicit confirmation. Blocking SHALL disable sending and receiving until the policy is changed.

#### Scenario: User blocks device
- **WHEN** the user confirms blocking a paired device
- **THEN** the device SHALL become blocked and send/auto-receive actions SHALL be disabled.

### Requirement: Android Maestro device trust validation

Device detail, trust badge, policy editor, save action, and destructive confirmation controls SHALL expose stable Android test IDs for Maestro validation.

#### Scenario: Maestro opens policy editor
- **WHEN** an Android Maestro flow opens a paired device detail and taps edit policy
- **THEN** the policy editor SHALL be visible by test ID.
