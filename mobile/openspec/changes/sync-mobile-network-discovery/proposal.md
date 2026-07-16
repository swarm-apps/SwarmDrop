## Why

Desktop SwarmDrop now automatically discovers LAN helper nodes and reports richer network candidate status. Mobile still treats bootstrap addresses as the main network setting, which makes the modern discovery model hard to explain or test.

## What Changes

- Add mobile startup config for discovery mode and LAN helper auto-discovery.
- Add mobile network status fields for candidate sources, LAN helper count, relay source, bootstrap readiness, and relay readiness.
- Redesign mobile Network settings around discovery behavior, with custom bootstrap nodes as an advanced fallback.
- Keep phone helper hosting off by default and not promoted as a primary action.
- Add Android Maestro validation for network settings.

## Capabilities

### New Capabilities

- `mobile-network-discovery-settings`: Mobile network discovery preferences and startup config.
- `mobile-network-status-projection`: Mobile network status presentation for bootstrap, relay, and LAN helper discovery.

### Modified Capabilities

None. No archived mobile network discovery capability exists.

## Impact

- `mobile-core`: network runtime config and status mirror changes.
- `src/stores/preferences-store.ts` and mobile core store/network state.
- `src/app/settings/network.tsx`, node control sheet, and status blocks.
- Android Maestro flow for network settings.
