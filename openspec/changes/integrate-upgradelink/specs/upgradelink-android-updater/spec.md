## ADDED Requirements

### Requirement: Android updater uses hybrid approach
The system SHALL use TypeScript SDK to fetch upgrade strategy, and AppUpdater library to execute APK installation on Android.

#### Scenario: SDK fetches upgrade strategy
- **GIVEN** the app is running on Android
- **WHEN** the updater checks for updates
- **THEN** it calls UpgradeLink TypeScript SDK to get strategy (version, upgradeType, downloadUrl)

#### Scenario: Strategy determines UI behavior
- **GIVEN** the SDK returns upgradeType = 'force'
- **WHEN** the app displays update dialog
- **THEN** it shows ForceUpdateDialog with no dismiss option

#### Scenario: Force upgrade blocks user
- **GIVEN** upgradeType is 'force'
- **WHEN** user tries to dismiss the update dialog
- **THEN** the dialog cannot be dismissed until update completes

#### Scenario: Prompt upgrade allows skip
- **GIVEN** upgradeType is 'prompt'
- **WHEN** the update dialog shows
- **THEN** user can click "Later" to dismiss and continue using app

#### Scenario: Silent upgrade in background
- **GIVEN** upgradeType is 'silent'
- **WHEN** the app checks for updates
- **THEN** it downloads APK in background without showing dialog

### Requirement: Rust bridges to Android for installation
The system SHALL provide a Rust Tauri command that invokes Android AppUpdater to install APK.

#### Scenario: Rust command triggers installation
- **GIVEN** user clicks "Update Now" in update dialog
- **WHEN** frontend calls `installAndroidUpdate(url)`
- **THEN** Rust command bridges to Kotlin and starts AppUpdater

#### Scenario: AppUpdater downloads and installs APK
- **GIVEN** AppUpdater receives download URL
- **WHEN** it starts downloading
- **THEN** it shows progress notification and installs APK when complete

#### Scenario: Installation requires permission
- **GIVEN** APK download completes
- **WHEN** system tries to install
- **THEN** if "Install unknown apps" permission is not granted, prompt user to enable it

### Requirement: Android version code mapping
The system SHALL map semantic version to Android versionCode for UpgradeLink comparison.

#### Scenario: Version conversion
- **GIVEN** app version is "0.1.2"
- **WHEN** checking for updates
- **THEN** it converts to versionCode 100012 (major * 10000 + minor * 100 + patch)
