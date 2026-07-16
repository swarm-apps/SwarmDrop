## Context

Received content on mobile should be discoverable after completion without making transfer history carry content semantics. Desktop already has inbox item summaries, detail files, missing flags, archive/delete, and repair helpers.

## Goals / Non-Goals

**Goals:**

- Show completed received content in a dedicated Inbox.
- Support mobile-appropriate open/share/copy/archive/delete actions.
- Preserve clear separation from Activity.

**Non-Goals:**

- No text/clipboard/bundle UI beyond displaying future content kind values gracefully.
- No cloud sync.
- No iOS validation requirement.

## Decisions

### D1. One inbox domain store

Use a dedicated inbox store for list/detail/loading/action state. Do not fold inbox content into transfer projections.

### D2. Share/open through platform mechanisms

Use Expo/platform mechanisms for opening or sharing received files. If direct open fails, copy the URI/path and show a recoverable message.

### D3. Destructive delete is explicit

Deleting an inbox record and deleting local files are separate user choices. Local file deletion requires destructive confirmation.

## Risks / Trade-offs

- [Risk] SAF/content URI behavior differs across Android providers. -> Reuse existing file access helpers and treat failures as missing-file states.
- [Risk] Inbox and Activity confusion. -> Keep labels and empty states content-focused.

## Migration Plan

1. Add native inbox bridge APIs.
2. Add inbox store.
3. Build Inbox tab list/detail/actions.
4. Add Android Maestro flows.
