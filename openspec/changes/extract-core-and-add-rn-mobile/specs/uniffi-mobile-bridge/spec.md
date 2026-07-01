## ADDED Requirements

### Requirement: UniFFI wrapper package
系统 SHALL provide `../swarmdrop-mobile/packages/swarmdrop-core` as a React Native native module package generated with `uniffi-bindgen-react-native`.

#### Scenario: Bridge package structure
- **WHEN** inspecting the bridge package
- **THEN** it contains a Rust wrapper crate, UniFFI build configuration, generated TypeScript bindings location, Android native module files, and iOS native module files or generation scripts

### Requirement: Wrapper crate isolates UniFFI
The UniFFI wrapper SHALL depend on `swarmdrop-core` and expose RN-friendly wrapper types without requiring UniFFI annotations inside `swarmdrop-core`.

#### Scenario: Core remains clean
- **WHEN** inspecting `crates/core`
- **THEN** core modules do not require `#[uniffi::export]` to compile
- **AND** UniFFI-specific records/enums/objects live in the mobile wrapper crate

### Requirement: RN event callback bridge
The UniFFI bridge SHALL expose an RN-implemented event callback interface that core events can be forwarded through.

#### Scenario: Pairing request arrives
- **WHEN** core emits an inbound pairing request event
- **THEN** the mobile wrapper maps it into a generated RN event type
- **AND** RN event bus code can update Zustand state or show a pairing request UI

### Requirement: MVP API surface
The UniFFI bridge SHALL expose only the APIs needed by the RN MVP before exposing advanced desktop-only features.

#### Scenario: MVP API audit
- **WHEN** reviewing generated TypeScript bindings for the first RN milestone
- **THEN** bindings include identity, network lifecycle, pairing, device listing, scan/prepare/send, accept/reject/cancel receive, and transfer history needed by mobile MVP
- **AND** bindings do not require MCP server APIs
