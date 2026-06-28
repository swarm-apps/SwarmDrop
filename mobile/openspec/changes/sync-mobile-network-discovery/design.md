## Context

The shared core has `DiscoveryMode`, candidate sources, LAN helper detection, relay readiness, and runtime infrastructure peer registration. Mobile should consume this as discovery status, not force users to manage Multiaddr lists by default.

## Goals / Non-Goals

**Goals:**

- Start mobile node with discovery runtime config.
- Show discovery health in human terms.
- Keep advanced bootstrap editing available.

**Non-Goals:**

- No mobile LAN helper hosting as a primary feature.
- No background service or battery policy work.
- No iOS validation requirement.

## Decisions

### D1. Discovery settings first, addresses second

Network settings focus on Auto vs LAN-only and LAN helper discovery. Custom bootstrap nodes move to an advanced section.

### D2. `provideLanHelper` remains false by default

The bridge can carry the field for core compatibility, but UI does not promote phone relay hosting.

### D3. Node status explains readiness

Status surfaces show public bootstrap, relay, LAN helper count, and relay source so users understand why devices are or are not reachable.

## Risks / Trade-offs

- [Risk] Users misunderstand LAN-only. -> Show copy that cross-network discovery may be limited.
- [Risk] Mobile OS limitations make helper hosting unreliable. -> Keep hosting out of primary UI.

## Migration Plan

1. Extend bridge config/status.
2. Extend preferences.
3. Redesign network settings and status surfaces.
4. Validate on Android with Maestro.
