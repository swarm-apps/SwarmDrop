## ADDED Requirements

### Requirement: Node Error State Visibility
The home (Devices) screen SHALL render a visually distinct state when the node runtime state is `error`, separate from the `stopped` state's headline, body text, and iconography.

#### Scenario: Node fails to start or crashes
- **WHEN** the node runtime state transitions to `error`
- **THEN** the home hero panel SHALL display a destructive-colored icon, an "error" headline distinct from the "stopped" headline, a summary of the failure, and a retry action

### Requirement: Bounded Information Density on Home Screen
While the node is running, the home screen SHALL NOT render more than 3 top-level panels simultaneously by default, and the add-device flow SHALL be collapsed behind a single entry point rather than always rendering its nearby-devices, pairing-code, and enter-code sub-panels.

#### Scenario: Node is running and the user has not started adding a device
- **WHEN** `runtimeState` is `running` and the user has not tapped an "add device" entry point
- **THEN** the home screen SHALL show at most 3 top-level panels (e.g. device overview, paired devices, active transfers), with the nearby-devices/pairing-code/enter-code sub-panels collapsed behind a single "add device" entry point

#### Scenario: User taps the add-device entry point
- **WHEN** the user taps the collapsed "add device" entry point
- **THEN** the app SHALL expand the nearby-devices, pairing-code, and enter-code sub-panels for that interaction

### Requirement: Pairing Request Recoverability
A pairing request in progress SHALL be cancellable by the user and SHALL automatically time out if no response is received, so the nearby device list is never left permanently disabled.

#### Scenario: User cancels an in-flight pairing request
- **WHEN** a pairing request is in progress and the user taps cancel on that device row
- **THEN** the app SHALL immediately re-enable the nearby device list and clear the pairing-in-progress state

#### Scenario: Peer never responds
- **WHEN** a pairing request has been in progress for more than 15 seconds without a response
- **THEN** the app SHALL automatically end the pairing attempt, re-enable the nearby device list, and show a "peer did not respond, try again" message

### Requirement: Accessible Send Action Labels
Each paired device row's send action SHALL expose an accessibility label that includes that device's display name, not a generic label shared across all rows.

#### Scenario: Screen reader user navigates the paired device list
- **WHEN** a screen reader focuses the send button on a device row
- **THEN** the accessibility label SHALL read "Send file to {device display name}" (localized), not a generic "Send file" label
