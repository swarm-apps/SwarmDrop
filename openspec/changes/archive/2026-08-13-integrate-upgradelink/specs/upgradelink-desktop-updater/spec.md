## ADDED Requirements

### Requirement: Desktop updater uses hybrid approach
The system SHALL use TypeScript SDK to fetch upgrade strategy, and Tauri updater to execute installation on desktop (Windows, macOS, Linux).

#### Scenario: SDK fetches upgrade strategy
- **GIVEN** the app is running on desktop
- **WHEN** the updater checks for updates
- **THEN** it calls UpgradeLink TypeScript SDK to get strategy (version, upgradeType, promptContent)

#### Scenario: Strategy determines UI behavior
- **GIVEN** the SDK returns upgradeType = 'force'
- **WHEN** the app displays update dialog
- **THEN** it shows ForceUpdateDialog blocking user interaction

#### Scenario: Force upgrade uses Tauri updater
- **GIVEN** user clicks "Update Now" in force dialog
- **WHEN** the app calls `check()` from `@tauri-apps/plugin-updater`
- **THEN** it downloads and installs the update, then restarts app

#### Scenario: Prompt upgrade allows skip
- **GIVEN** upgradeType is 'prompt'
- **WHEN** the update dialog shows
- **THEN** user can click "Remind Me Later" to skip this version

#### Scenario: Silent upgrade in background
- **GIVEN** upgradeType is 'silent'
- **WHEN** the app checks for updates
- **THEN** it downloads update in background and installs on next restart

### Requirement: Desktop maintains signature verification
The system SHALL verify update package signatures using Tauri's built-in signature verification.

#### Scenario: Valid signature
- **GIVEN** an update package with valid signature
- **WHEN** Tauri updater downloads the package
- **THEN** signature verification passes and installation proceeds

#### Scenario: Invalid signature
- **GIVEN** an update package with invalid signature
- **WHEN** Tauri updater verifies the package
- **THEN** the update is rejected and error is shown to user

### Requirement: Desktop updater endpoints point to GitHub Releases
The system SHALL configure Tauri updater endpoints to download actual files from GitHub Releases.

#### Scenario: Update file download
- **GIVEN** UpgradeLink returns download URL
- **WHEN** Tauri updater downloads the file
- **THEN** it fetches from GitHub Releases (or custom CDN) directly
