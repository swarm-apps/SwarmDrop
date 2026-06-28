## ADDED Requirements

### Requirement: Network discovery settings foundation

The Settings surface SHALL reserve a clear network discovery section for discovery mode, LAN helper discovery, and advanced bootstrap nodes. Native runtime config and status sync SHALL be implemented by `sync-mobile-network-discovery`.

#### Scenario: User opens Network settings
- **WHEN** the user opens Network settings
- **THEN** discovery controls placement and advanced bootstrap placement SHALL be visible or clearly represented.

#### Scenario: User opens advanced bootstrap
- **WHEN** the user opens advanced network options
- **THEN** custom bootstrap nodes SHALL remain available as an advanced fallback, not the primary network workflow.

### Requirement: Network status presentation slots

Node status surfaces SHALL have presentation slots for bootstrap readiness, relay readiness, LAN helper count, candidate sources, relay source, and connected/discovered peers. Missing native fields SHALL degrade to concise unavailable copy until `sync-mobile-network-discovery` lands.

#### Scenario: Native field unavailable
- **WHEN** the current bridge does not provide a discovery status field
- **THEN** the UI SHALL avoid fake values and show a neutral unavailable or pending-sync state.

### Requirement: Mobile helper hosting is not primary

The foundation UI SHALL NOT promote the phone as a LAN helper or relay server by default. Any helper hosting control, if present later, SHALL be hidden behind advanced settings and default to off.

#### Scenario: Fresh install network settings
- **WHEN** a user opens Network settings on a fresh install
- **THEN** the app SHALL show discovery controls placement but SHALL NOT show helper hosting as a primary toggle.
