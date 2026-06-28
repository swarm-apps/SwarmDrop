## ADDED Requirements

### Requirement: Mobile network runtime config

The mobile native bridge SHALL accept network runtime config fields for custom bootstrap nodes, discovery mode, auto-discover LAN helpers, and local LAN helper provision flag. Mobile UI SHALL default the provision flag to false in this change.

#### Scenario: Start node with auto discovery
- **WHEN** RN starts the node with discovery mode auto and LAN helper discovery enabled
- **THEN** native SHALL pass those options to shared core startup.

#### Scenario: Start node in LAN-only mode
- **WHEN** RN starts the node with discovery mode lanOnly
- **THEN** native SHALL start without public bootstrap candidates and SHALL rely on local discovery paths.

### Requirement: Network discovery settings

The Network settings surface SHALL expose mobile-safe controls for automatic discovery mode and LAN helper discovery. Manual bootstrap node editing SHALL be available as an advanced fallback, not the primary path.

#### Scenario: User changes discovery mode
- **WHEN** the user changes discovery mode
- **THEN** the app SHALL persist the setting and indicate whether node restart is required.

#### Scenario: User edits bootstrap nodes
- **WHEN** the user opens advanced bootstrap settings
- **THEN** the app SHALL allow adding or removing custom multiaddrs without making that the default workflow.

### Requirement: Network status visibility

The app SHALL show network status fields relevant to mobile users: node status, connected peers, discovered peers, bootstrap readiness, relay readiness, LAN helper count, candidate sources, relay source, and local helper running state if native reports it.

#### Scenario: Relay ready through LAN helper
- **WHEN** network status reports relay ready and relay source is a LAN helper
- **THEN** the UI SHALL label the relay path as local helper assisted.

#### Scenario: No bootstrap connected
- **WHEN** network status reports no bootstrap connected
- **THEN** the UI SHALL show a concise troubleshooting hint.

### Requirement: Mobile does not expose helper hosting as a primary action

The foundation UI SHALL NOT promote the phone as a LAN helper or relay server by default. Any helper hosting control, if present for debugging, SHALL be hidden behind advanced settings and default to off.

#### Scenario: Fresh install network settings
- **WHEN** a user opens Network settings on a fresh install
- **THEN** the app SHALL show discovery controls but SHALL NOT show helper hosting as a primary toggle.

### Requirement: Android Maestro network validation

Network settings and node status surfaces SHALL expose stable Android test IDs for discovery mode, LAN helper discovery, advanced bootstrap, and status blocks.

#### Scenario: Maestro verifies discovery settings
- **WHEN** an Android Maestro flow opens Network settings
- **THEN** discovery controls and advanced bootstrap entry SHALL be visible by test ID.
