## 1. Accessible primary contrast (token, do first — unblocks nothing but is the lowest-risk, widest-impact change)

- [x] 1.1 Update light-mode `--primary-foreground` in `src/global.css` to the dark-ink value already used for the same purpose in dark mode (`222.2 47.4% 11.2%`), so it clears ≥4.5:1 against `--primary`
- [x] 1.2 Update DESIGN.md's "Flipped Ink Rule" (Colors section Named Rules + Do's/Don'ts) to describe the new unified (no-longer-flipped) light/dark behavior
- [x] 1.3 Update `.impeccable/design.json` narrative `rules` entry for the renamed rule
- [x] 1.4 Visual sweep of all `bg-primary`/`text-primary-foreground` surfaces app-wide in light mode (not limited to the three critiqued screens) to confirm no regressions

## 2. Devices surface hardening (home / Devices tab)

- [x] 2.1 Read `src/app/(main)/index.tsx`'s `HomeTransferPanel` and `runtimeState` derivation; add a distinct `error` branch (destructive icon, "node error" headline distinct from "stopped", failure summary, retry action)
- [x] 2.2 Refactor `AddDevicePanel` into a collapsed single entry point that expands the nearby-devices/pairing-code/enter-code sub-panels on tap, so the running-state home screen shows ≤3 top-level panels by default
- [x] 2.3 Investigate whether the native `requestPairing` binding already implements a timeout (Open Question in design.md) before adding a client-side one
- [x] 2.4 Add a cancel affordance to the in-flight pairing row and a 15s client-side timeout that re-enables the nearby device list and shows a "peer did not respond" message
- [x] 2.5 Update `DeviceCard` (`row` and `card` variants) send-button `accessibilityLabel` to include the device's display name instead of a generic "发送文件"

## 3. Inbox integrity hardening

- [x] 3.1 Read `src/stores/inbox-store.ts`'s `loadDetail`/`refresh`/`runSearch` error handling to determine whether load-failure can be distinguished from confirmed not-found (Open Question in design.md)
- [x] 3.2 Implement the distinguishable error-vs-deleted states in `src/app/inbox/[itemId].tsx` per the design's decision (precise dual-state if distinguishable now, otherwise the "surface `lastError` + more cautious copy" fallback)
- [x] 3.3 Surface `lastError` visibly (not just `console.warn`) on refresh failures in `src/app/(main)/inbox.tsx` and search failures in `src/app/inbox/search.tsx`
- [x] 3.4 Consolidate `FilterRail` (`inbox-list.tsx`, shared by `inbox.tsx` and `search.tsx`) to ≤4 primary chips plus a "more filters" entry for archived/anomalous
- [x] 3.5 Preserve the active content-type filter when entering keyword search in `search.tsx`, and add the missing/archived/AI-agent badges to `InboxHitRow` so it matches `InboxRow`
- [x] 3.6 Replace `ContentPreview`'s decorative icon-only block with an inline truncated text excerpt for text/clipboard/multi-file content types (image previews unchanged)
- [x] 3.7 Fix the inbox detail title (`text-[22px]`) and toolbar item count (`text-[26px]`) to use the documented Title (15px) or Headline (24px) type scale from DESIGN.md

## 4. Settings hardening

- [x] 4.1 Replace the static Lucide `Loader` icon with `ActivityIndicator` for the "checking for updates" state in `src/app/settings/about.tsx`
- [x] 4.2 Fix `NetworkHint`'s hardcoded `color="#f59e0b"` to read from `useThemeColors().warning` in `src/app/settings/network.tsx`
- [x] 4.3 Read `network.tsx`'s existing state fields to define a "network status: good/limited" synthesis heuristic that needs no new native binding fields (Open Question in design.md)
- [x] 4.4 Implement the collapsed network diagnostics: default synthesized status line + "view diagnostic details" disclosure gating NAT/candidate-nodes/LAN-Helper/relay/bootstrap fields
- [x] 4.5 Downgrade the "reset default receive location" confirmation in `general.tsx` from destructive to neutral styling
- [x] 4.6 Add a lightweight confirmation and a completion toast to bootstrap node removal in `bootstrap-nodes.tsx`
- [x] 4.7 Unify `rounded-xl` → `rounded-lg` across `SettingSection` (`setting-row.tsx`), `DeviceInfoCard`, and bootstrap-nodes card containers

## 5. Verification

- [x] 5.1 `pnpm typecheck` and `pnpm lint` (biome check `src/`) pass
- [x] 5.2 Manual or Maestro walkthrough of every changed state: node error, pairing cancel/timeout, inbox error-vs-deleted, filter rail, content preview, network diagnostics disclosure, confirmation strength, primary-button contrast (see session notes: several states verified live on Android emulator; node-error/pairing-timeout/inbox states require fault injection or a second peer/seeded data and were verified via code review instead)
- [ ] 5.3 Re-run `/impeccable critique` on `src-app-main-index-tsx`, `src-app-main-inbox-tsx`, `src-app-main-settings-tsx`; confirm score improvement over the 20/22/24 baseline and that the prior P0/P1 findings no longer reproduce
- [ ] 5.4 `openspec validate harden-mobile-primary-surfaces --strict` passes before archiving
