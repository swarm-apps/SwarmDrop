## ADDED Requirements

### Requirement: Mobile onboarding
RN mobile SHALL provide onboarding that initializes the core, sets a device name, and optionally guides pairing.

#### Scenario: First mobile launch
- **WHEN** the user opens the RN app for the first time
- **THEN** onboarding allows them to set or confirm a device name
- **AND** completes without requiring a desktop Tauri webview or password vault

### Requirement: Mobile pairing
RN mobile SHALL support pairing through generated code and input code flows compatible with desktop SwarmDrop.

#### Scenario: Mobile pairs with desktop
- **WHEN** desktop generates a pairing code and mobile enters it
- **THEN** mobile resolves the peer, requests pairing, and stores the paired device through shared core behavior

#### Scenario: Desktop pairs with mobile
- **WHEN** mobile generates a pairing code and desktop enters it
- **THEN** desktop resolves the peer, requests pairing, and both sides show paired device state

### Requirement: Mobile device and network state
RN mobile SHALL display current network status and paired/nearby device state from core events and queries.

#### Scenario: Device becomes online
- **WHEN** a paired desktop device becomes reachable
- **THEN** RN mobile updates its device list to show the online state without requiring app restart

### Requirement: Mobile foreground send
RN mobile SHALL support sending selected files to a paired online device while the app remains in the foreground.

#### Scenario: Mobile sends file to desktop
- **WHEN** mobile user selects a supported file and sends it to a paired desktop device
- **THEN** desktop receives an offer
- **AND** after desktop accepts, transfer progress and completion are shown on mobile

### Requirement: Mobile foreground receive
RN mobile SHALL support accepting an incoming transfer offer while the app remains in the foreground.

#### Scenario: Mobile receives file from desktop
- **WHEN** desktop sends a file to mobile and mobile accepts
- **THEN** mobile writes the file to the configured MVP save location
- **AND** mobile shows progress and completion
