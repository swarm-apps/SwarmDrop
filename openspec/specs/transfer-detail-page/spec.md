# transfer-detail-page Specification

## Purpose
TBD - created by archiving change add-transfer-offer-dialog-and-detail-page. Update Purpose after archive.
## Requirements
### Requirement: Display transfer detail page
The system SHALL display a dedicated page for viewing transfer progress and details at the route `/transfer/:sessionId`.

#### Scenario: Navigate to transfer detail
- **WHEN** the user navigates to `/transfer/:sessionId` or is redirected there after accepting an offer
- **THEN** the system SHALL display the transfer detail page
- **AND** the page SHALL show the device name of the peer
- **AND** the page SHALL show a file tree with transfer status for each file

### Requirement: Show real-time progress
The system SHALL display real-time transfer progress for active transfers.

#### Scenario: Transfer in progress
- **WHEN** the transfer is in "transferring" status
- **THEN** the page SHALL display an overall progress bar
- **AND** the page SHALL show current transfer speed
- **AND** the page SHALL show estimated time remaining
- **AND** the page SHALL highlight the currently transferring file in the file tree

### Requirement: Show completed state
The system SHALL display a success state when the transfer is completed.

#### Scenario: Transfer completed
- **WHEN** the transfer status changes to "completed"
- **THEN** the page SHALL display a success icon (green checkmark)
- **AND** the page SHALL show "所有文件发送完成" or "所有文件接收完成" message
- **AND** the page SHALL show statistics: file count, total size, duration
- **AND** the page SHALL show all files in the file tree with completed status
- **AND** the page SHALL provide a "打开文件夹" button for received files

### Requirement: Show failed state
The system SHALL display an error state when the transfer fails.

#### Scenario: Transfer failed
- **WHEN** the transfer status changes to "failed"
- **THEN** the page SHALL display an error icon
- **AND** the page SHALL show the error message
- **AND** the page SHALL provide a "重试" button if applicable

### Requirement: Cancel active transfer
The system SHALL allow the user to cancel an active transfer.

#### Scenario: User cancels transfer
- **WHEN** the transfer is in "transferring" status
- **THEN** the page SHALL provide a "取消传输" button
- **WHEN** the user clicks the cancel button
- **THEN** the system SHALL call `cancelSend()` or `cancelReceive()` as appropriate
- **AND** the transfer status SHALL change to "cancelled"

### Requirement: Responsive design
The system SHALL adapt the detail page layout for mobile and desktop viewports.

#### Scenario: Mobile viewport
- **WHEN** the viewport width is less than 768px
- **THEN** the page SHALL use a stacked layout optimized for mobile
- **AND** the file tree SHALL be scrollable

#### Scenario: Desktop viewport
- **WHEN** the viewport width is 768px or greater
- **THEN** the page SHALL use a side-by-side layout with sidebar
- **AND** the file tree and progress SHALL be visible simultaneously

