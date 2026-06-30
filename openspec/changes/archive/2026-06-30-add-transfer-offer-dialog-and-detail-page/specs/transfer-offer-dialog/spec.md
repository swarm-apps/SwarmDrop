## ADDED Requirements

### Requirement: Display transfer offer dialog
The system SHALL display a modal dialog when a `transfer-offer` event is received, without navigating away from the current page.

#### Scenario: Offer dialog appears
- **WHEN** the system receives a `transfer-offer` event
- **THEN** a modal dialog SHALL be displayed overlaying the current page
- **AND** the dialog SHALL show the sender's device name
- **AND** the dialog SHALL show a file tree preview of the files being offered
- **AND** the dialog SHALL show the default save path
- **AND** the dialog SHALL provide "接收" (Accept) and "拒绝" (Reject) buttons

### Requirement: Handle multiple offers
The system SHALL queue multiple transfer offers and display them sequentially.

#### Scenario: Multiple offers queued
- **WHEN** the system receives a `transfer-offer` event while another offer dialog is open
- **THEN** the new offer SHALL be added to the pending offers queue
- **AND** the next offer dialog SHALL be displayed after the current dialog is closed

### Requirement: Accept offer action
The system SHALL start the transfer and navigate to the detail page when the user accepts an offer.

#### Scenario: User accepts the offer
- **WHEN** the user clicks the "接收" (Accept) button
- **THEN** the system SHALL call `acceptReceive()` with the session ID and save path
- **AND** the system SHALL add the session to `transfer-store`
- **AND** the system SHALL navigate to `/transfer/:sessionId`

### Requirement: Reject offer action
The system SHALL reject the transfer when the user clicks reject.

#### Scenario: User rejects the offer
- **WHEN** the user clicks the "拒绝" (Reject) button or closes the dialog
- **THEN** the system SHALL call `rejectReceive()` with the session ID
- **AND** the dialog SHALL close
- **AND** the system SHALL show the next pending offer if any

### Requirement: Change save path
The system SHALL allow the user to change the save path before accepting.

#### Scenario: User changes save path
- **WHEN** the user clicks the "更改" (Change) button
- **THEN** a folder picker dialog SHALL open
- **AND** the selected path SHALL become the new save path for the transfer
