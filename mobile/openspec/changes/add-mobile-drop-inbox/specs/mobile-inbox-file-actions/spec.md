## ADDED Requirements

### Requirement: File open/share action

The mobile app SHALL provide an open or share action for available received inbox files.

#### Scenario: File available
- **WHEN** the user taps an available inbox file
- **THEN** the app SHALL invoke the platform open/share mechanism.

### Requirement: Missing file marking

The app SHALL mark an inbox file as missing when opening or sharing fails because the file is not found.

#### Scenario: File deleted externally
- **WHEN** a file no longer exists at its stored location
- **THEN** the app SHALL mark it missing and keep the inbox record visible.

### Requirement: Delete semantics

Inbox deletion SHALL distinguish deleting only the record from deleting both the record and local files.

#### Scenario: Delete record only
- **WHEN** the user confirms deleting an inbox record without local file deletion
- **THEN** the record SHALL be removed and local files SHALL remain.
