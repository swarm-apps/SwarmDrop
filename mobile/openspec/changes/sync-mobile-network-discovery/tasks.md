## 1. Bridge

- [ ] 1.1 Add mobile discovery mode enum and runtime config record.
- [ ] 1.2 Extend `startNode` bridge to pass discovery config.
- [ ] 1.3 Extend mobile network status mirror with candidate and LAN helper fields.

## 2. Preferences and Store

- [ ] 2.1 Add discovery mode preference.
- [ ] 2.2 Add auto-discover LAN helpers preference.
- [ ] 2.3 Keep provide-LAN-helper off by default.
- [ ] 2.4 Preserve custom bootstrap nodes as advanced settings.

## 3. UI

- [ ] 3.1 Redesign Network settings around discovery controls.
- [ ] 3.2 Move bootstrap node editing to advanced section.
- [ ] 3.3 Update node control/status sheet with candidate and relay source information.
- [ ] 3.4 Add restart-required hints for running-node config changes.

## 4. Validation

- [ ] 4.1 Add Android Maestro flow for network settings.
- [ ] 4.2 Run `pnpm typecheck`.
- [ ] 4.3 Run `openspec validate sync-mobile-network-discovery --strict`.
