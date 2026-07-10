## ADDED Requirements

### Requirement: Fixed task navigation and primary actions

Mobile send, Offer, transfer detail, and inbox detail surfaces SHALL keep task navigation and primary actions outside the middle content scroll area. Primary actions SHALL remain reachable on small screens, in landscape, with large text, and when system Safe Area insets are present.

#### Scenario: Long transfer detail on a short screen
- **WHEN** transfer summary and file content exceed the available screen height
- **THEN** the middle content SHALL scroll while the task header and applicable transfer actions remain reachable

#### Scenario: Bottom system inset
- **WHEN** the device reports a bottom Safe Area inset
- **THEN** the fixed action area SHALL include sufficient bottom padding and SHALL NOT be obscured by the home indicator or system navigation area

### Requirement: Single vertical scroll owner

Each task surface SHALL have at most one primary vertical scroll owner. A virtualized file or device list SHALL NOT be nested inside a same-axis ScrollView, and a large file collection SHALL NOT be rendered by mapping all rows inside an outer ScrollView.

#### Scenario: Transfer detail includes summary and files
- **WHEN** transfer detail renders summary, progress, diagnostics, and a large file collection
- **THEN** the non-file content SHALL participate in the same virtualized scroll surface as the file rows and the action bar SHALL remain outside it

#### Scenario: Share Target has files and devices
- **WHEN** the system Share Target must show both shared content and a device list
- **THEN** the device list SHALL remain the primary scroll owner and the full shared-file collection SHALL be inspected in a separate FileBrowser surface rather than a competing nested list

### Requirement: Responsive Offer container

Incoming Offer SHALL use a near-full-height bottom sheet or equivalent full-screen mobile container on phones and a wide constrained dialog on tablet-sized screens. Both containers SHALL render the same Offer content and FileBrowser behavior.

#### Scenario: Phone receives a large Offer
- **WHEN** a phone receives an Offer containing more files than fit on screen
- **THEN** the source summary and decision actions SHALL remain reachable while the complete virtualized file collection scrolls in the middle

#### Scenario: Tablet receives an Offer
- **WHEN** a device at or above the tablet width threshold receives an Offer
- **THEN** the app SHALL use a wider centered container without duplicating Offer policy, save-location, file action, or decision logic

### Requirement: Offer queue state resets between items

Transient Offer UI state SHALL be scoped to the current Offer. Changing to the next queued Offer SHALL reset save-location override, list position, directory expansion, and transient view state while preserving the stored transfer-scope view preference.

#### Scenario: User handles the first queued Offer
- **WHEN** the first Offer is accepted, rejected, or dismissed and the next queued Offer becomes current
- **THEN** the next Offer SHALL open at the top with no save-location override or expanded directories inherited from the previous Offer

### Requirement: Predictable content under dynamic sizing

Dynamic summaries, errors, progress, keyboard appearance, and font scaling SHALL reduce or scroll the middle content area rather than pushing primary actions off-screen or shrinking the file area to an unusable sliver.

#### Scenario: Large text expands Offer metadata
- **WHEN** accessibility font scaling makes Offer metadata and policy text taller
- **THEN** the middle region SHALL remain scrollable and the reject/accept controls SHALL remain visible and operable

#### Scenario: Preparation progress replaces send actions
- **WHEN** send preparation displays hashing or scanning progress in the fixed footer
- **THEN** the footer SHALL remain Safe Area aware and the file collection SHALL retain the remaining flexible height
