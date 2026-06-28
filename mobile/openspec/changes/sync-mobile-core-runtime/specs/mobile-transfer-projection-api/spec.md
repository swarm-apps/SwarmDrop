## ADDED Requirements

### Requirement: Transfer projection records

The mobile bridge SHALL expose transfer projection records with phase, suspended reason, terminal reason, recoverable flag, epoch, policy action, policy reason, save location, and file projections.

#### Scenario: Projection list
- **WHEN** RN calls the projection list API
- **THEN** native SHALL return all known projections in a stable newest-first order.

### Requirement: Projection event delivery

The mobile bridge SHALL emit transfer projection update events when the shared core publishes projection changes.

#### Scenario: Coordinator publishes update
- **WHEN** the shared core coordinator updates a transfer projection
- **THEN** RN SHALL receive an event containing the full updated projection.

### Requirement: Old history API removal

The bridge SHALL NOT require `listTransferHistory`, `MobileSessionStatus`, or old history item types for new transfer UI.

#### Scenario: RN transfer store migration
- **WHEN** the new RN transfer store is implemented
- **THEN** it SHALL consume projection APIs rather than old history APIs.
