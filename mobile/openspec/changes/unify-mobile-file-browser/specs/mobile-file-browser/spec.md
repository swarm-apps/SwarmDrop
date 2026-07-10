## ADDED Requirements

### Requirement: Unified file collection model

The mobile app SHALL adapt selected files, transfer offers, transfer projections, and inbox files into one FileBrowser item model before rendering a file collection. Tree and grid views SHALL consume the same leaf item collection and SHALL NOT maintain separate business state.

#### Scenario: Switch between tree and grid
- **WHEN** the user switches a file collection from tree to grid or back
- **THEN** the same leaf files, statuses, progress values, and available actions SHALL remain present

#### Scenario: Different sources share a filename
- **WHEN** two selected files have the same filename and relative path but different source identifiers
- **THEN** both files SHALL remain independently selectable, removable, and sendable

#### Scenario: Same source is selected twice
- **WHEN** the same source identifier is added more than once to the current selection
- **THEN** the app SHALL retain exactly one selection entry for that source

### Requirement: Stable identity is independent of display path

The mobile app SHALL use source identity for selected files, session-scoped file identity for Offer/projection files, and native inbox file identity for inbox files. `relativePath` SHALL be used for hierarchy and display only and SHALL NOT be the sole leaf identity.

#### Scenario: Same file ID in different sessions
- **WHEN** two transfer sessions both contain a file with the same numeric file ID
- **THEN** their FileBrowser item identifiers SHALL remain distinct

#### Scenario: Remove a directory by path boundary
- **WHEN** the user removes directory `foo/`
- **THEN** files under `foo/` SHALL be removed and files under sibling path `foobar/` SHALL remain

### Requirement: Projection-first transfer status

The mobile app SHALL keep `MobileTransferProjection` as the durable transfer fact source and MAY overlay live progress by file ID. FileBrowser SHALL NOT introduce or persist a second JS-owned session projection.

#### Scenario: Terminal progress overlay is cleared
- **WHEN** a transfer reaches a terminal state and its high-frequency progress snapshot is removed from the RN store
- **THEN** FileBrowser SHALL continue to render every file and its terminal status from the retained native projection

#### Scenario: Completed transfer
- **WHEN** a projection terminates as completed
- **THEN** every projected file SHALL render as completed

#### Scenario: Failed transfer with partially completed files
- **WHEN** a projection terminates with a fatal error after some files completed
- **THEN** completed files SHALL remain completed and incomplete files SHALL render as error

#### Scenario: Suspended transfer
- **WHEN** a projection is suspended after one file made partial progress
- **THEN** completed files SHALL remain completed, the partial file SHALL render as paused, and untouched files SHALL render as waiting

### Requirement: Explicit file actions

FileBrowser SHALL expose remove-file, remove-directory, open, reveal, and retry behavior only through callbacks supplied by its caller. FileBrowser SHALL NOT infer actions from the current route, view mode, or file source.

#### Scenario: Read-only Offer
- **WHEN** FileBrowser renders an incoming Offer without action callbacks
- **THEN** it SHALL show the complete file collection without remove, open, reveal, or retry controls

#### Scenario: Inbox file action
- **WHEN** an available inbox file is rendered with the existing open callback
- **THEN** activating the file SHALL preserve the platform open/share behavior defined by the inbox capability

### Requirement: Tree and grid views

FileBrowser SHALL provide tree and grid views. Tree SHALL derive directory nodes from normalized relative paths and expose expanded state; grid SHALL render leaf files directly. Directory expansion SHALL NOT be represented as persistent selection highlighting.

#### Scenario: Expand a directory
- **WHEN** the user expands a directory in tree view
- **THEN** its descendants SHALL become visible and expanded state SHALL be conveyed by accessibility state, disclosure icon, and hierarchy rather than selection styling

#### Scenario: Flat file sources
- **WHEN** every file source only provides a filename with no directory hierarchy
- **THEN** tree view SHALL render a flat file list and SHALL NOT fabricate directories

### Requirement: Scoped view preferences

The mobile app SHALL persist FileBrowser view preferences independently for `send`, `transfer`, and `inbox` scopes. Send SHALL default to tree, transfer SHALL default to tree, and inbox SHALL default to grid.

#### Scenario: Change inbox view
- **WHEN** the user switches the inbox FileBrowser to tree
- **THEN** send and transfer view preferences SHALL remain unchanged

#### Scenario: Upgrade existing preferences
- **WHEN** persisted preferences predate FileBrowser scopes or contain an invalid view value
- **THEN** the app SHALL merge valid existing preferences and apply the default for each missing or invalid FileBrowser scope

### Requirement: Virtualized large collections

Tree and grid views SHALL use a virtualized list and SHALL NOT mount the complete file collection at once. FileBrowser SHALL support collections of 1, 100, 1,000, and 10,000 files without falling back to `map` rendering.

#### Scenario: Ten thousand files
- **WHEN** FileBrowser receives 10,000 leaf files
- **THEN** only a bounded visible window SHALL be mounted and the user SHALL be able to reach the final file

#### Scenario: Switch view on a large collection
- **WHEN** the user switches tree/grid view or the grid column count changes
- **THEN** the new list SHALL start at a predictable top position and SHALL NOT reuse an incompatible pixel offset

### Requirement: Preview permission boundary

FileBrowser SHALL render a thumbnail only from an explicit adapter-provided `previewUri`. It SHALL NOT derive preview access from source identifiers, local paths, or relative paths.

#### Scenario: Incoming Offer before acceptance
- **WHEN** FileBrowser renders files from an Offer that has not been accepted
- **THEN** every file SHALL use a type icon and the app SHALL NOT attempt to open or preview a remote path

#### Scenario: Thumbnail load fails
- **WHEN** an explicit preview URI cannot be loaded or its permission has expired
- **THEN** the card SHALL fall back to a file type icon without continuously retrying the failed image

### Requirement: All mobile file collection surfaces use FileBrowser

Interactive send selection, system Share Target file review, incoming Offer, transfer detail, and multi-file inbox detail SHALL use the unified FileBrowser model and renderers. These surfaces SHALL NOT retain private file-row collections after migration.

#### Scenario: Open each file collection surface
- **WHEN** the user visits any supported file collection surface
- **THEN** its file identity, status, view preference, accessibility, and actions SHALL be provided through FileBrowser and its adapters

#### Scenario: Single-file inbox media preview
- **WHEN** inbox detail renders an existing single-file rich media preview
- **THEN** that preview SHALL remain available and SHALL preserve its existing open/share behavior without treating the preview itself as a second file collection

### Requirement: Accessible file browsing

FileBrowser SHALL expose accessible names for view controls and file actions, selected state for the active view, expanded state for directories, and readable file name, size, status, and progress for file items. Interactive touch targets SHALL meet the project's mobile minimum target size.

#### Scenario: Screen reader inspects an active transfer file
- **WHEN** a screen reader focuses a transferring file
- **THEN** it SHALL announce the file name, size, transferring status, and current progress
