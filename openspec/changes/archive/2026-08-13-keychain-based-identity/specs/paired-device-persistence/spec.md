## ADDED Requirements

### Requirement: Backend-persisted paired devices
The system SHALL persist paired device metadata in backend-owned storage and SHALL load it during backend initialization.

#### Scenario: Startup loads paired devices
- **WHEN** the backend initializes device/network state
- **THEN** it loads persisted paired devices into `DeviceManager`
- **AND** the frontend is not required to hydrate paired devices from Stronghold before network startup

### Requirement: Pairing persists new device
The system SHALL persist a newly paired device before treating it as durable app state.

#### Scenario: Pairing accepted
- **WHEN** a pairing request is accepted and produces `PairedDeviceInfo`
- **THEN** the backend stores the paired device in persistent storage
- **AND** updates `DeviceManager`
- **AND** emits device state changes for the frontend

### Requirement: Removing paired device updates persistence
The system SHALL remove paired devices from backend persistent storage when the user unpairs a device.

#### Scenario: User removes paired device
- **WHEN** the user removes a paired device
- **THEN** the backend deletes that device from persistent storage
- **AND** removes it from `DeviceManager`
- **AND** emits device state changes for the frontend

### Requirement: Legacy paired device migration
The system SHALL migrate legacy paired devices from Stronghold into backend-owned storage during the identity migration process.

#### Scenario: Legacy paired devices migrated
- **WHEN** legacy Stronghold migration succeeds and the vault contains paired devices
- **THEN** the system stores those paired devices in backend persistent storage
- **AND** avoids creating duplicate records for devices that already exist
