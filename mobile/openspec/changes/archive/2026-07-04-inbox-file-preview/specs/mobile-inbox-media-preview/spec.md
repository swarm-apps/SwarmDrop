## ADDED Requirements

### Requirement: Fullscreen image viewer

For a single-file inbox item whose inline image preview is rendered, the app SHALL let the user open that image in an in-app fullscreen viewer with pinch-to-zoom, without leaving the app.

#### Scenario: Tap inline image preview

- **WHEN** the user taps the inline image preview on the inbox detail page
- **THEN** the app SHALL show the image fullscreen with pinch-to-zoom, dismissible by gesture or a close control back to the detail page.

### Requirement: Inline video playback

For a single-file inbox item whose only file is an available local (`file://`) video, the app SHALL render an inline video player (native controls, no autoplay) in the detail page preview slot.

#### Scenario: Video item detail

- **WHEN** the user opens the inbox detail of a single available `file://` video file
- **THEN** the detail page SHALL render a playable inline video component, and playback SHALL start only after the user taps the play control.

### Requirement: Media preview applicability

The app SHALL NOT render inline media preview when the item is multi-file, the file is missing, or its localPath is not `file://`; the type-icon chip SHALL be shown instead and the open action SHALL still use the system preview/open mechanism.

#### Scenario: Non-applicable item

- **WHEN** the inbox item is multi-file, missing, or stored behind a non-`file://` URI (Android SAF directory)
- **THEN** the app SHALL show the type-icon chip without an inline media preview, and tapping open SHALL hand the file to the system.
