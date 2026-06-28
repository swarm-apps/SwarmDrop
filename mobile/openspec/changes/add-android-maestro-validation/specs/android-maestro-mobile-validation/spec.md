## ADDED Requirements

### Requirement: Android-only Maestro suite

The repo SHALL provide Maestro flows for Android validation of primary mobile UX. iOS Maestro validation SHALL NOT be required by this capability.

#### Scenario: Run Android suite
- **WHEN** the Android Maestro suite is run against a built app
- **THEN** it SHALL validate primary navigation and critical visible states without requiring iOS.

### Requirement: Stable test IDs

Primary tabs, screens, empty states, policy editor, network settings, inbox detail, activity sections, and destructive confirmations SHALL expose stable test IDs.

#### Scenario: Text changes
- **WHEN** localized visible text changes
- **THEN** Maestro flows SHALL continue to target stable test IDs where possible.

### Requirement: Surface validation flows

The suite SHALL include flows for shell navigation, empty Devices/Inbox/Activity states, device trust editor, network discovery settings, and receive offer presentation when fixture state is available.

#### Scenario: Navigation flow
- **WHEN** the navigation flow runs
- **THEN** Devices, Inbox, Activity, and Settings surfaces SHALL each become visible.

### Requirement: Validation documentation

The repo SHALL document how to run the Android Maestro suite locally and where artifacts are written.

#### Scenario: Developer runs docs command
- **WHEN** a developer follows the documented Android Maestro command
- **THEN** the suite SHALL run or fail with actionable setup guidance.
