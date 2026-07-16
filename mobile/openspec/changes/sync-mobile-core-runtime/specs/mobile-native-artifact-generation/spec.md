## ADDED Requirements

### Requirement: Generated binding refresh

The implementation SHALL regenerate TypeScript and C++ binding files after bridge record or method changes.

#### Scenario: API checksum check
- **WHEN** the app loads the native module
- **THEN** generated API checksum validation SHALL pass for every exposed method.

### Requirement: Android artifact smoke

The implementation SHALL run an Android build or native module load smoke that proves regenerated artifacts are usable on Android.

#### Scenario: Android app starts
- **WHEN** the Android app starts after artifact regeneration
- **THEN** the native SwarmDrop mobile core module SHALL load without checksum or missing-symbol errors.
