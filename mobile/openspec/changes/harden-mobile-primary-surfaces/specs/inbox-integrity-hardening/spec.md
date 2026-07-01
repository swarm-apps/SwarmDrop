## ADDED Requirements

### Requirement: Distinguish Load Failure from Deleted Record
The inbox item detail screen SHALL NOT present the same message for a transient load failure as for a confirmed record deletion.

#### Scenario: Detail load throws a transient error
- **WHEN** loading item detail fails due to an exception (not a confirmed not-found result)
- **THEN** the screen SHALL show a retry-capable error state reflecting the underlying store error, not the "record may have been deleted" message

#### Scenario: Record legitimately no longer exists
- **WHEN** the backend confirms the record does not exist
- **THEN** the screen SHALL show the existing "record may have been deleted" message, without a retry action

### Requirement: Store Errors Surfaced to the User
Inbox list refresh and search failures SHALL surface the store's error state to the user, not only log it silently.

#### Scenario: Refresh fails
- **WHEN** the inbox list `refresh()` call catches an error
- **THEN** the inbox list SHALL display a visible error indicator derived from that error, in addition to any console logging

#### Scenario: Search fails
- **WHEN** a keyword search request fails
- **THEN** the search screen SHALL display a visible error state distinct from the "no results found" empty state

### Requirement: Bounded Filter Choices
The inbox filter rail SHALL present no more than 4 primary filter options at the top level; any remaining filters SHALL be reachable through a secondary "more filters" entry point.

#### Scenario: User opens the inbox filter rail
- **WHEN** the filter rail renders
- **THEN** at most 4 filter chips SHALL be visible at the top level, with additional filters (e.g. archived, anomalous) accessible via a "more filters" affordance

### Requirement: Filter Consistency Across Search
Entering keyword search SHALL preserve the user's previously selected content-type filter, and search result rows SHALL carry the same status badges as the browse list.

#### Scenario: User searches while a filter is active
- **WHEN** the user has a non-default filter selected and enters a search query
- **THEN** the search results SHALL be scoped to that filter, or the UI SHALL clearly indicate the filter scope was expanded

#### Scenario: Search result corresponds to a missing or archived item
- **WHEN** a search hit corresponds to an item that is missing or archived
- **THEN** the search result row SHALL display the same missing/archived/AI-agent badges shown in the browse list

### Requirement: Honest Content Preview
Non-image inbox item previews (text, clipboard, multi-file) SHALL show an inline excerpt of the actual content rather than a purely decorative icon block.

#### Scenario: User opens a text or clipboard item
- **WHEN** the opened item's content type is text or clipboard
- **THEN** the detail screen SHALL render a truncated inline excerpt of the actual received text within the preview area, not only an icon and caption

### Requirement: Typography Within Documented Scale
Inbox detail and toolbar text SHALL use only the type sizes documented in DESIGN.md's typography roles, not undocumented arbitrary pixel values.

#### Scenario: Rendering the inbox detail title and toolbar item count
- **WHEN** the detail title or toolbar item count is rendered
- **THEN** their font sizes SHALL match a documented DESIGN.md typography role (Title 15px or Headline 24px), not an undocumented arbitrary value such as 22px or 26px
