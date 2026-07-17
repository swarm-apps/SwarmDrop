## ADDED Requirements

### Requirement: Discovery startup config

Mobile node startup SHALL accept discovery mode, LAN helper auto-discovery, custom bootstrap nodes, and provide-LAN-helper flag.

#### Scenario: Auto discovery start
- **WHEN** the node starts in auto discovery mode
- **THEN** native SHALL pass public bootstrap and LAN helper discovery config to shared core.

### Requirement: Network settings controls

Network settings SHALL expose discovery mode and LAN helper discovery as primary controls, with custom bootstrap nodes in an advanced area.

#### Scenario: User opens network settings
- **WHEN** the user opens Network settings
- **THEN** discovery controls SHALL be visible before manual bootstrap editing.

### Requirement: Helper hosting is not primary

Mobile SHALL default helper hosting to off and SHALL NOT promote it as a primary network action.

#### Scenario: Fresh install
- **WHEN** the user opens Network settings after a fresh install
- **THEN** helper hosting SHALL be off and not shown as a primary toggle.
