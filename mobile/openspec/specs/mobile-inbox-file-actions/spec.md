# mobile-inbox-file-actions Specification

## Purpose
TBD - created by archiving change inbox-file-preview. Update Purpose after archive.
## Requirements
### Requirement: File open/share action

The mobile app SHALL open available received inbox files with the platform's system preview/open mechanism (iOS: QuickLook preview; Android: ACTION_VIEW handoff to installed apps via a content URI), and SHALL provide sharing as an explicit secondary action instead of the primary one.

#### Scenario: File available

- **WHEN** the user taps an available inbox file (a multi-file row, or the single-file primary "打开" action)
- **THEN** the app SHALL present the file via the system preview/open mechanism, not the share sheet.

#### Scenario: No system handler

- **WHEN** the system cannot open the file (no installed app handles its type)
- **THEN** the app SHALL fall back to the system share sheet, and SHALL surface an error toast if sharing is also unavailable.

#### Scenario: Share as secondary action

- **WHEN** the user chooses the share action from the detail actions sheet for a single available file
- **THEN** the app SHALL invoke the system share sheet for that file.

