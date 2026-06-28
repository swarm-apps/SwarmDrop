## ADDED Requirements

### Requirement: Bottom-tab primary shell

The mobile app SHALL expose Devices, Inbox, and Activity as the primary authenticated navigation surfaces. Settings SHALL be reachable as a secondary action and SHALL NOT be a primary tab.

#### Scenario: Returning user opens app
- **WHEN** a user has completed onboarding and opens the app
- **THEN** the app SHALL show the primary mobile shell with Devices, Inbox, and Activity navigation visible.

#### Scenario: User switches between primary tasks
- **WHEN** the user taps Devices, Inbox, or Activity
- **THEN** the app SHALL switch surfaces without opening a drawer or modal.

### Requirement: Devices tab is the sending and pairing home

The Devices tab SHALL show paired devices, nearby pairable devices or pairing actions, node status, and active transfer summary. The primary action for an online paired device SHALL be sending files to that device.

#### Scenario: Online paired device
- **WHEN** an online paired device is visible on Devices
- **THEN** its row or card SHALL show status, trust badge, connection hint, and a send action.

#### Scenario: No paired devices
- **WHEN** the user has no paired devices
- **THEN** Devices SHALL show an empty state with a direct pairing call to action.

### Requirement: Mobile visual foundation

The app shell SHALL use consistent mobile primitives for headers, bottom tabs, cards or rows, badges, bottom sheets, dialogs, and empty states. Touch targets for interactive controls MUST be at least 44 pt, and layout spacing SHALL follow an 8 point grid.

#### Scenario: Primary action placement
- **WHEN** a screen has a single primary action
- **THEN** the action SHALL be reachable in the lower half of the screen or in a bottom sheet action area.

#### Scenario: Empty state presentation
- **WHEN** a primary surface has no data
- **THEN** it SHALL show an icon, concise title, helpful secondary text, and a direct next action when one exists.

### Requirement: Android Maestro navigation hooks

Primary navigation elements and top-level screens SHALL expose stable Android test IDs for Maestro validation.

#### Scenario: Maestro opens each tab
- **WHEN** an Android Maestro flow taps each primary tab by test ID
- **THEN** each corresponding screen SHALL become visible with a stable screen test ID.
