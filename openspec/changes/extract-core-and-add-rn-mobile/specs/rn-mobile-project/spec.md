## ADDED Requirements

### Requirement: RN project scaffold
系统 SHALL create a sibling React Native project at `../swarmdrop-mobile` based on Expo and the SwarmNote Mobile architecture.

#### Scenario: RN project exists
- **WHEN** inspecting the workspace after scaffold
- **THEN** `../swarmdrop-mobile/package.json`, `app.json`, `src/app`, `src/core`, `src/stores`, and `packages/swarmdrop-core` exist

### Requirement: RN app stack
The RN app SHALL use Expo Router, React Native, Zustand, Lingui, NativeWind or equivalent RN styling, and RN-compatible primitive UI components.

#### Scenario: RN dependencies installed
- **WHEN** inspecting `../swarmdrop-mobile/package.json`
- **THEN** dependencies include Expo, Expo Router, React Native, Zustand, Lingui, RN navigation/screens dependencies, and the local `react-native-swarmdrop-core` package

### Requirement: Development build only
The RN app SHALL be documented and configured as an Expo development build app, not an Expo Go-only app.

#### Scenario: Native bridge dependency exists
- **WHEN** the RN project includes `react-native-swarmdrop-core`
- **THEN** project documentation states that native development builds are required
- **AND** Expo Go is not presented as a supported runtime for core features

### Requirement: Mobile app initialization
The RN app SHALL initialize i18n, persisted stores, secure identity provider, event bus and UniFFI core before routing into the main app.

#### Scenario: RN app boots
- **WHEN** the RN root layout starts
- **THEN** it waits for core initialization and onboarding hydration before deciding whether to show onboarding or the main mobile flow
