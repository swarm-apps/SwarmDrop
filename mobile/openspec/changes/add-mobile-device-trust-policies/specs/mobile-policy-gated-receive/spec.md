## ADDED Requirements

### Requirement: Auto-accept policy context

The incoming receive UX SHALL show when a transfer was auto-accepted due to device policy.

#### Scenario: Owned device auto-accept
- **WHEN** native auto-accepts an incoming offer
- **THEN** the app SHALL show or record policy context rather than prompting for manual acceptance.

### Requirement: Policy rejection context

Policy-rejected incoming offers SHALL show a clear rejection reason and SHALL NOT show accept controls.

#### Scenario: Blocked peer offer
- **WHEN** native rejects an offer because the peer is blocked
- **THEN** the app SHALL show a policy rejection message.

### Requirement: Confirmation required path

Offers that require confirmation SHALL still show the manual accept/reject dialog with trust and policy summary.

#### Scenario: Collaborator offer
- **WHEN** a collaborator sends an offer and policy requires confirmation
- **THEN** the app SHALL show accept and reject actions plus policy context.
