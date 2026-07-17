## ADDED Requirements

### Requirement: Collapsed Network Diagnostics
The network settings screen SHALL show one synthesized status summary by default and SHALL move protocol-level diagnostic fields (NAT status, candidate nodes, LAN helper, relay, bootstrap nodes) behind a "view diagnostic details" disclosure.

#### Scenario: User opens network settings
- **WHEN** the network settings screen loads
- **THEN** it SHALL show a single synthesized network status line by default, with the full set of protocol-level fields hidden until the user taps "view diagnostic details"

#### Scenario: User expands diagnostic details
- **WHEN** the user taps "view diagnostic details"
- **THEN** the screen SHALL reveal the protocol-level fields previously hidden

### Requirement: Honest In-Progress Indicators
Any "in progress" state (e.g. checking for updates) SHALL be represented with an animating indicator, not a static icon.

#### Scenario: Checking for updates
- **WHEN** the update check is in progress
- **THEN** the about screen SHALL show an animating activity indicator consistent with the indicator used elsewhere in the app, not a static, non-rotating icon

### Requirement: Confirmation Strength Matches Risk
Destructive-styled confirmation SHALL be reserved for actions that are irreversible or affect connectivity/data; reversible, local-only preference actions SHALL use a neutral confirmation instead.

#### Scenario: Resetting the default receive location
- **WHEN** the user resets the receive folder to its default
- **THEN** the app SHALL show a neutral (non-destructive-styled) confirmation, since the action is fully reversible and local-only

#### Scenario: Removing a custom bootstrap node
- **WHEN** the user removes a custom bootstrap node
- **THEN** the app SHALL request at least a lightweight confirmation and SHALL show a completion toast, since the action can affect connectivity

### Requirement: Semantic Color Tokens Only
Status-conveying colors (e.g. warning icons) SHALL be sourced from the theme color tokens, never hardcoded hex literals.

#### Scenario: Rendering the network warning hint
- **WHEN** the network hint component renders its warning icon
- **THEN** its color SHALL be read from the theme's warning color token, not a hardcoded hex string

### Requirement: Documented Corner-Radius Tokens Only
Settings surface containers SHALL use only the corner-radius values documented in DESIGN.md (sm/md/lg/full), not undocumented values.

#### Scenario: Rendering a settings section card
- **WHEN** a setting section, device info card, or bootstrap-nodes card container renders
- **THEN** its corner radius SHALL match a DESIGN.md-documented token (e.g. `rounded-lg` / 10px), not an undocumented value such as `rounded-xl`
