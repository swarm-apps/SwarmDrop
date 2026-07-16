## ADDED Requirements

### Requirement: WCAG AA Contrast for Primary-Filled Surfaces
Text and icons rendered on top of the `--primary` background color SHALL meet or exceed a 4.5:1 contrast ratio in both light and dark color schemes.

#### Scenario: Light mode primary button
- **WHEN** a button or badge uses `bg-primary` with `text-primary-foreground` in light mode
- **THEN** the contrast ratio between the text/icon color and the primary background SHALL be at least 4.5:1

#### Scenario: Dark mode primary button (regression guard)
- **WHEN** the same component renders in dark mode
- **THEN** the contrast ratio SHALL likewise be at least 4.5:1
