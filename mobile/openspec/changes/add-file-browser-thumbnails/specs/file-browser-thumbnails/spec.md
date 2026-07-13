## ADDED Requirements

### Requirement: Image thumbnails in grid mode

The file-browser grid SHALL render an image thumbnail (cover-fit) in place of the generic file-type icon for any image file that has an on-device local (`file://`) path, in both the send and inbox scopes. The thumbnail SHALL be downscaled to the cell size rather than decoded at full resolution.

#### Scenario: Send-scope image shows a thumbnail

- **WHEN** the send-prepare grid contains an image file whose local source (`file://`) path is available
- **THEN** the cell SHALL show that image cover-fit, instead of the image-type icon

#### Scenario: Inbox-scope image shows a thumbnail

- **WHEN** a received, completed multi-file inbox item is shown in grid mode and one of its files is a local image
- **THEN** that file's cell SHALL show the image thumbnail

#### Scenario: Portrait photo displays upright

- **WHEN** the source image carries EXIF orientation metadata
- **THEN** the thumbnail SHALL be displayed upright, not rotated or sideways

### Requirement: Video thumbnails in grid mode

The file-browser grid SHALL render a first-frame poster thumbnail carrying a visible play indicator for any video file that has an on-device local (`file://`) path, in both the send and inbox scopes.

#### Scenario: Send-scope video shows a poster + play badge

- **WHEN** the send-prepare grid contains a local video file
- **THEN** its cell SHALL show a generated first-frame poster overlaid with a play badge

#### Scenario: Inbox-scope video shows a poster + play badge

- **WHEN** a completed inbox grid contains a local video file
- **THEN** its cell SHALL show a first-frame poster overlaid with a play badge

#### Scenario: A video is visually distinguishable from a still image

- **WHEN** a cell displays a video poster
- **THEN** it SHALL carry a visible play indicator so the user can tell a video from a still image

### Requirement: Icon fallback for un-thumbnailable files

The grid SHALL fall back to the file-type icon — never a broken or empty image cell — whenever a thumbnail cannot be produced: no available local file, an unsupported (non-image, non-video) type, a decode or generation failure, or a missing file.

#### Scenario: No available local file

- **WHEN** a grid item has no available local (`file://`) path
- **THEN** the cell SHALL show the file-type icon

#### Scenario: Video generation fails

- **WHEN** first-frame generation for a video fails (unsupported codec, HDR, or no decodable frame)
- **THEN** the cell SHALL fall back to the file-type icon without surfacing an error

#### Scenario: Image cannot be decoded on this platform

- **WHEN** an image format cannot be decoded on the current platform/OS version (e.g. HEIC on an older Android)
- **THEN** the cell SHALL fall back to the file-type icon

### Requirement: Bounded, cached thumbnail generation

Video poster generation SHALL be cached and bounded: a generated poster SHALL be reused on re-display without re-decoding, generation SHALL be concurrency-limited, and repeated scrolling of the grid SHALL NOT grow the on-disk thumbnail cache without bound.

#### Scenario: Cached poster is reused

- **WHEN** a video cell that already generated a poster is scrolled off-screen and back
- **THEN** the cached poster SHALL be shown without regenerating it

#### Scenario: Generation is concurrency-limited

- **WHEN** many video cells become visible at once during a fast scroll
- **THEN** concurrent poster generation SHALL be limited to a small number of operations rather than one per visible cell simultaneously

### Requirement: Thumbnail scope applicability

Grid thumbnails SHALL apply only to the send and inbox scopes in this change; the transfer (live progress) and offer (pre-download) scopes SHALL continue to show file-type icons.

#### Scenario: Live transfer grid is unchanged

- **WHEN** the live transfer grid is shown
- **THEN** cells SHALL show file-type icons alongside status and progress, not thumbnails

#### Scenario: Incoming offer grid is unchanged

- **WHEN** an incoming offer's file list is shown before the transfer is accepted or downloaded
- **THEN** cells SHALL show file-type icons

### Requirement: Recycled cell correctness

On the virtualized/recycled grid, a cell SHALL NOT display a previous item's thumbnail after being reused for a different file.

#### Scenario: Fast-scroll recycling shows no stale thumbnail

- **WHEN** grid cells are rapidly recycled while scrolling
- **THEN** each cell SHALL show its own file's thumbnail or icon, never a stale thumbnail carried over from the item that previously occupied the cell
