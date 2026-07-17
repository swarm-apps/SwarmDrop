## Context

Policy belongs to the relationship with a device. Mobile should make that relationship visible where users choose send/receive behavior, not bury it in global settings.

## Goals / Non-Goals

**Goals:**

- Display trust on device surfaces.
- Edit receive policy per paired device.
- Present policy decisions on incoming offers.

**Non-Goals:**

- No account-level ownership proof.
- No full MCP permission dashboard.
- No compatibility with global auto-accept as the main model.

## Decisions

### D1. Device detail owns policy editing

Device cards lead to detail; detail exposes send and edit-policy actions. This avoids accidental file picker launches and gives trust enough context.

### D2. Bottom sheet policy editor

Use a bottom sheet with segmented trust selection and progressive disclosure for advanced fields. This fits mobile thumb ergonomics better than a desktop-form copy.

### D3. Unknown devices default to collaborator

Legacy or missing trust fields are displayed and treated as collaborator, requiring confirmation by default.

## Risks / Trade-offs

- [Risk] Users may mark a risky device as owned. -> Show plain-language copy and visible badges.
- [Risk] Advanced policy fields overload the sheet. -> Collapse advanced controls by default.

## Migration Plan

1. Add bridge types and update command.
2. Extend paired device cache.
3. Add device detail and trust badges.
4. Add policy editor and incoming offer policy copy.
5. Validate on Android with Maestro.
