## ADDED Requirements

### Requirement: Register as a system share target

The app SHALL appear as a target in the OS share sheet for **file, image, and video** content on both iOS (Share Extension) and Android (`ACTION_SEND` / `ACTION_SEND_MULTIPLE`). Unsupported content types (plain text, web URL) SHALL NOT enter the send flow in this version.

#### Scenario: File shared from another app lists SwarmDrop

- **WHEN** a user selects a file, image, or video in another app and opens the system share sheet
- **THEN** SwarmDrop appears as a share target
- **AND** choosing it hands the shared item(s) to SwarmDrop

#### Scenario: Unsupported content type is out of scope

- **WHEN** a user shares plain text or a web URL
- **THEN** SwarmDrop does not route it into the send flow (not registered for those types in this version)

### Requirement: Normalize shared content to app-owned files

When content is shared in, the system SHALL copy each shared item into app-owned storage and expose it as a `file://` path together with its display name and size. The transfer pipeline SHALL read from this app-owned copy so a long transfer does not depend on the sharing app's transient content-URI permission.

#### Scenario: Android content URI becomes an app-owned file

- **WHEN** Android hands a `content://` shared item to the app
- **THEN** the item is copied to an app-owned `file://` path before sending
- **AND** the transfer reads the `file://` copy, not the original `content://`

#### Scenario: Multiple shared files are all normalized

- **WHEN** the user shares multiple files at once (`ACTION_SEND_MULTIPLE`)
- **THEN** every shared item is normalized to an app-owned `file://` entry with name and size

### Requirement: Route a share to the target-device screen

Upon receiving a supported share, the app SHALL present a target-device screen that shows a summary of the shared files (count and total size) and a list of paired devices that are currently online and sendable. This SHALL work whether the app was launched by the share (cold start) or was already running (warm start).

#### Scenario: Warm start pushes the target-device screen

- **WHEN** the app is already open and a supported share arrives
- **THEN** the target-device screen is pushed on top of the current screen
- **AND** an in-progress transfer, if any, is not interrupted

#### Scenario: Cold start opens the target-device screen once ready

- **WHEN** the app is launched by a share while not running
- **THEN** after the app finishes initializing it opens the target-device screen with the shared files

### Requirement: Send shared files to a chosen device

From the target-device screen, selecting a device SHALL send the shared files to it through the existing send pipeline and navigate to the transfer progress screen for the new session.

#### Scenario: Tapping an online device starts the transfer

- **WHEN** the user taps an online, sendable device on the target-device screen
- **THEN** the shared files are sent to that device
- **AND** the app navigates to the transfer progress screen for the new session

### Requirement: Auto-start the node when a share needs it

If the P2P node is not running when a share is received, the app SHALL start it (showing a loading state) so devices can be discovered, without requiring the user to leave the share flow.

#### Scenario: Node stopped at share time

- **WHEN** a share is received while the node is not running
- **THEN** the target-device screen starts the node and shows progress
- **AND** once running, online sendable devices appear for selection

### Requirement: Handle no reachable devices

If no paired device is currently online and sendable, the target-device screen SHALL show an empty state guiding the user to pair a device or bring one online, and SHALL retain the shared files so no re-share is needed once a device becomes available.

#### Scenario: No online device

- **WHEN** the target-device screen has no online sendable paired device
- **THEN** it shows an empty state with pairing / bring-online guidance
- **AND** the shared files remain available when a device comes online

### Requirement: Defer a share received before onboarding

If a supported share arrives before onboarding is complete, the app SHALL notify the user to finish setup first and route them into onboarding. The current share is NOT queued in this version; the user re-shares after setup.

#### Scenario: Share into a fresh install

- **WHEN** a share is received before onboarding is complete
- **THEN** the app shows a notice to finish setup and opens onboarding
- **AND** it does not open the target-device screen for this share (no stash/resume in this version)
