## ADDED Requirements

### Requirement: Discovery status fields

Mobile network status SHALL include connected peers, discovered peers, bootstrap readiness, relay readiness, LAN helper count, candidate sources, relay source, and local helper running state if available.

#### Scenario: Relay ready
- **WHEN** relay readiness is true
- **THEN** the UI SHALL show relay as ready and include source context when available.

### Requirement: Troubleshooting copy

Network status UI SHALL provide concise hints when bootstrap or relay readiness is missing.

#### Scenario: No bootstrap
- **WHEN** bootstrap readiness is false
- **THEN** the UI SHALL show a brief reachability hint.
