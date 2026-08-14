## ADDED Requirements

### Requirement: CI publishes updates to UpgradeLink
The system SHALL automatically sync release information to UpgradeLink after a successful GitHub Release.

#### Scenario: Successful release triggers sync
- **GIVEN** a GitHub Actions workflow completes a Tauri release
- **WHEN** the release is published to GitHub Releases
- **THEN** the workflow invokes UpgradeLink Action to sync the update metadata

#### Scenario: Sync includes correct version
- **GIVEN** the release version is v0.1.2
- **WHEN** UpgradeLink receives the sync request
- **THEN** the version v0.1.2 is registered in UpgradeLink platform

### Requirement: CI sync uses official UpgradeLink Action
The system SHALL use the official `toolsetlink/upgradelink-action` for CI integration.

#### Scenario: Action configuration
- **GIVEN** the GitHub Actions workflow runs
- **WHEN** the upgradeLink-upload job executes
- **THEN** it uses `toolsetlink/upgradelink-action@3.0.2` with proper credentials

### Requirement: CI waits for release completion
The system SHALL ensure the release job completes before attempting UpgradeLink sync.

#### Scenario: Job dependency
- **GIVEN** the publish-tauri job is running
- **WHEN** it completes successfully
- **THEN** the upgradeLink-upload job starts with the correct version output
