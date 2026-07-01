---
target: devices page
total_score: 23
p0_count: 1
p1_count: 3
timestamp: 2026-07-01T07-45-08Z
slug: src-routes-app-devices
---
Method: dual-agent (A: a4d61b2f8292bf86b · B: a5c9ed6356acd3539)

## Design Health Score

| # | Heuristic | Score | Key Issue |
|---|-----------|-------|-----------|
| 1 | Visibility of System Status | 3 | Node pill, countdown, spinners, online dots are good; trust/pending badge disappears once a connection badge appears; disabled "发送" gives no reason. |
| 2 | Match System / Real World | 3 | Plain Chinese product language throughout — until a raw Kademlia DHT error string leaks straight into a user-facing toast. |
| 3 | User Control and Freedom | 3 | Cancel/close available everywhere; regenerate-code is a nice "undo." No dead-ends found. |
| 4 | Consistency and Standards | 2 | Ad hoc `glass-control` buttons diverge from shadcn `Button` focus/hover treatment; the "selected" pill color literally differs between light (black) and dark (blue) for the same state. |
| 5 | Error Prevention | 2 | OTP length-gating is good, but a wrong/expired code isn't prevented from producing a raw technical dead end, with no format hint beforehand. |
| 6 | Recognition Rather Than Recall | 2 | No inline legend for trust levels or connection-type abbreviations; duplicate generic "Device" names force recall over recognition. |
| 7 | Flexibility and Efficiency of Use | 2 | Whole-card-click is a nice shortcut, but no keyboard shortcuts, no bulk trust-policy actions, no search over the paired list. |
| 8 | Aesthetic and Minimalist Design | 3 | Genuinely restrained composition, docked by 6-hue badge sprawl and an unintentional dead-space gap. |
| 9 | Help Users Recognize/Diagnose/Recover from Errors | 1 | The DHT error toast is the textbook failure: technical, mixed-language, no recovery guidance, no persistent in-context state. |
| 10 | Help and Documentation | 2 | No tooltips/info affordances anywhere for trust levels or connection types. |
| **Total** | | **23/40** | **Acceptable — significant improvements needed before users are happy** |

## Anti-Patterns Verdict

**Start here.** Does this look AI-generated? Not on first glance — no gradient hero, no cheerful mascot, no side-stripe borders, no gradient text. The failure mode here isn't generic AI slop, it's a design system that documents rules it doesn't actually enforce on this page.

**LLM assessment (Assessment A, live-inspected):** The glass-chrome system is real and mostly honored for structural panels, but DESIGN.md's own "Flat-Control, Glass-Chrome Rule" ("never apply `backdrop-filter` to a button or input") is violated repeatedly on this exact screen. The "One Accent Rule" is violated at scale — six saturated-ish hues (blue/green/emerald/amber/red/zinc) doing semantic-status duty where the spec calls for one accent plus destructive red. The hero-metric-tiles template (3 KPI-style stat cards above the fold) is present, softened by glass. Worst of all, live-testing the pairing-code flow with an invalid code surfaced a raw libp2p/Kademlia Rust error string verbatim in a toast — the single most damning "nobody looked at this before shipping" signal found in the review.

**Deterministic scan (Assessment B):** The bundled CLI detector (`detect.mjs`) returned **zero findings** against both the devices route and `section-primitives.tsx` — confirmed genuine (not a broken/misconfigured scan) via a `--no-config` re-run and a synthetic sanity-check file containing known offenders, which the detector correctly caught. Manual file:line review against the fixed 7-category checklist found no side-stripe borders, no gradient text, no uppercase eyebrows, no numbered markers, and clean accessible-attribute coverage (aria-labels on all icon-only buttons, proper label association on the one form input). It **did** independently confirm the glass-on-buttons violation with exact locations: `device-card.tsx:234` (Send button), `device-card.tsx:248` (Connect button), `device-card.tsx:294` (overflow-menu trigger), `add-device-section.tsx:222` (copy-code button), `add-device-section.tsx:236` (regenerate-code button). It also flagged two dialog-description text-overflow risks and a cluster of 10-11px colored status text worth a dedicated contrast pass.

**Where they agree / where the detector has a blind spot:** The CLI detector's clean run and Assessment A's "violates its own documented rules" finding aren't actually in conflict — the detector's registry (side-tab, gradient-text, hero-eyebrow-chip, numbered-section-markers, low-contrast, etc.) is generic and cross-project; it has no rule for "violates *this project's own* DESIGN.md contract." Assessment B's manual read-through filled that gap and landed on the exact same violation Assessment A found live, independently, with precise file:line evidence neither could have produced alone. **No visual overlay/screenshot exists from Assessment B** — browser evidence was deliberately skipped on its end to avoid a race condition on the single native Tauri window Assessment A was actively driving; that inspection was covered by Assessment A's live screenshots instead.

## Overall Impression

The information architecture is the strongest thing here — paired devices genuinely get primary visual weight over discovery/pairing, a faithful execution of PRODUCT.md's "隐式优先于选择" principle, not just lip service. But the page doesn't yet enforce the design system it documents: glass chrome bleeds onto buttons, one accent becomes six, and roughly half the custom interactive controls ship with no focus-visible state at all. The single biggest opportunity is trust: this product's whole pitch is "no cloud middleman, encrypted, worth trusting with your files" — and right now the one moment that pitch is tested hardest (a mistyped pairing code) ends in a raw Rust/DHT error dump, and the one signal that should never disappear (an unconfirmed peer's trust state) vanishes the moment it connects.

## What's Working

1. **Implicit-first IA is genuinely well-executed.** Paired devices — the thing you actually act on — get the primary column and largest visual weight; discovery/pairing is correctly demoted to a secondary aside. A faithful, non-trivial implementation of the product's core design principle.
2. **The pairing-code cell is the one place the DESIGN.md "signature component" promise actually lands** — `font-mono`, tabular digits, tight tracking, glass-control chip, live countdown. It reads exactly like the "trust-me-with-a-secret" moment the design system says it should be.
3. **Empty/loading states are implemented with real care, not an afterthought** — "还没有已配对设备," "暂无附近设备" (distinguishing "filtered to empty" from "genuinely empty"), and "暂无正在传输" all pair a title with a next-step description.

## Priority Issues

**[P0] Raw backend/libp2p internals leak into the user-facing pairing error.**
Why it matters: Live-reproduced — typing a wrong/expired 6-digit code throws a toast reading `Kad error: GetRecord: NotFound { key: Key(b"...")​, closest_peers: [PeerId("12D3KooW...")] }`, verbatim Rust/DHT internals in English inside an otherwise all-Chinese product. Worse, the OTP dialog never enters a visible error state, so once the toast auto-dismisses the user is left staring at the same digits with no idea anything happened. Mistyped codes are an extremely common real path, and for a product whose whole pitch is trustworthiness, this is the worst possible place to fail.
Fix: Map the backend `AppError.kind` for DHT lookup failures to a friendly, localized message ("未找到该设备，请确认配对码正确且尚未过期"); add a persistent inline error state to the OTP dialog (red-ring + message) instead of relying solely on a transient toast.
Suggested command: `/impeccable harden`

**[P1] The trust/pending-confirmation badge disappears exactly when it's most safety-relevant.**
Why it matters: In `device-card.tsx`, the card footer shows *either* the connection-type badge (LAN/DCUtR/Relay) *or* the trust badge (which carries "待确认") — never both. The moment an unconfirmed device connects, its pending/trust indicator vanishes, so a device you haven't vetted yet looks identical to a fully-trusted one while it's live — exactly the state PRODUCT.md's "状态诚实可见" principle says should never be hidden.
Fix: Always show trust/pending status (e.g. a small persistent icon by the device name), independent of the connection badge; move connection-type info to a secondary/tooltip slot.
Suggested command: `/impeccable harden`

**[P1] The design system's own "Flat-Control, Glass-Chrome Rule" is violated on this exact page.**
Why it matters: `glass-control` (`backdrop-filter: blur(12px)`) sits directly on 5 clickable controls — confirmed at `device-card.tsx:234` (Send), `device-card.tsx:248` (Connect), `device-card.tsx:294` (overflow trigger), `add-device-section.tsx:222` (copy-code), `add-device-section.tsx:236` (regenerate-code) — directly contradicting DESIGN.md's explicit "never apply backdrop-filter to a button or input." Confirmed independently by both the live design review and a static file:line read, so this isn't a one-off slip.
Fix: Strip `glass-*` classes from every button on this page; apply the documented flat `shadow-xs` treatment; reserve glass strictly for non-interactive containers.
Suggested command: `/impeccable audit`

**[P1] Custom interactive controls ship with no visible focus state.**
Why it matters: The device card itself (`role="button"`), the overflow-menu trigger, the copy/regenerate-code buttons, and the nearby-device row all lack any `focus-visible:` class — unlike the shadcn `Button` instances on the same screen, which correctly ship `focus-visible:ring-ring/50`. That's roughly half the interactive surface with inconsistent or possibly absent keyboard-focus indication, directly against PRODUCT.md's stated WCAG AA bar. (Live keyboard testing in this session was inconclusive due to automation limits; this finding is source-code-verified via the missing className pattern, not a live screenshot.)
Fix: Add the same `focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[3px]` treatment used by shadcn `Button` to every custom clickable element on this page.
Suggested command: `/impeccable audit`

**[P2] One Accent Rule violated at scale, with an associated contrast risk.**
Why it matters: Green (online dot, LAN badge), amber (temporary trust, relay badge), emerald (owned trust), and red (blocked trust) all appear alongside Signal Blue — six hues doing status duty where DESIGN.md calls for one accent plus destructive-only red. Several of these live as 10-11px colored text (`text-green-500`, `text-[10px]` trust/connection badges) that hasn't been contrast-checked and is exactly the size/weight combination most likely to fail 4.5:1.
Fix: Consolidate status semantics onto shape/icon/text differentiation with a single accent color; reserve color for destructive/blocked states only; run a contrast pass on the surviving colored text.
Suggested command: `/impeccable colorize` (contrast verification as a follow-up via `/impeccable audit`)

**[P2] Live-verified dead space breaks the panel's own composition.**
Why it matters: The paired-devices `SectionShell` (`min-h-full`) is grid-stretched to match the height of the aside's add-device panel. With only 6 devices (2 grid rows), this leaves ~300-400px of empty glass chrome before "正在传输" appears — confirmed in both light and dark screenshots at 1360×1000. This gets worse, not better, with fewer paired devices.
Fix: Stop forcing equal-height stretch between independently-sized sibling panels; size each section to its own content.
Suggested command: `/impeccable layout`

## Persona Red Flags

**Jordan (confused first-timer) — pairing a new device via code:**
- Types a wrong/expired code → gets the raw `Kad error: GetRecord: NotFound {...}` toast (live-reproduced). No first-timer parses this as "try again" — they parse it as "something is broken."
- After the toast auto-dismisses, the OTP fields still show the failed code with zero visual error state — no idea whether to retype, wait, or ask the other person for a fresh code.
- "本机配对码" and "输入配对码" sit in the same panel separated only by a 1px hairline — nothing narrates which side to use when. Jordan has to infer the two-sided nature of pairing from layout proximity alone.
- Once paired, Jordan is never told what trust level was applied — the safe default (`collaborator` → requires manual confirmation) is invisible.

**Sam (accessibility-dependent, keyboard/screen-reader):**
- Source-verified: the device card, overflow trigger, copy/regenerate buttons, and nearby-device row all lack `focus-visible:` classes, unlike shadcn `Button` instances on the same page. Roughly half the interactive surface has inconsistent or possibly absent keyboard-focus indication.
- The disabled "发送" button on an offline card gives no `aria-describedby`/reason — a screen-reader user hears "发送, disabled" with no explanation.
- Trust/connection badges do pair icon + short text (not color-only) — this specific pattern works fine for Sam.

**Riley (stress tester, edge cases):**
- Live-observed: 3 of 6 paired devices are labeled generically "Device" with an identical icon — plausible under real-world conditions (generic hostnames, corporate imaging), with no secondary differentiator like the last 4 chars of the peer ID.
- Deliberately probing the error path (garbage/expired pairing code) immediately surfaces unmapped backend internals — exactly what a stress tester tries first.
- The overflow-menu dropdown visually overlapped a neighboring card's header when triggered from certain grid positions (live-observed) — worth testing near the window edge / 3rd column.

**"隐私敏感用户" (privacy-conscious user, derived from PRODUCT.md):**
- The backend's default trust policy for a fresh pairing (`autoAccept:false, requireConfirmation:true`) is actually the right safe default — credit where due.
- But this default and its consequences are never surfaced at the pairing moment itself; a privacy-conscious user has no way to know, without separately opening 信任策略 afterward, that incoming files will require manual confirmation.
- The P1 trust-badge-hides-when-connected finding above is *the* red flag for this persona specifically — the one moment they'd most want a visible "not fully vetted yet" signal is exactly when the UI hides it.

## Minor Observations

- Two dialog descriptions interpolate the device name into fixed-width dialogs (`trust-policy-dialog.tsx:134-136`, `device-card.tsx:389-393`) with no `truncate`/`break-words` guard — an unbreakable long hostname could overflow the dialog rather than wrap.
- `device-card.tsx:136` uses `opacity-72`, which isn't in Tailwind's default opacity scale — likely silently generates no CSS, so the intended dimming of non-interactive/offline cards may not actually apply. Worth a quick verification.
- The "selected"/"primary action" visual language is inconsistent: the nearby-device filter's active pill is `bg-zinc-950` (black) in light mode but `bg-blue-500/20` in dark mode for the identical state; separately, the "发送"/"配对" pill in `NearbyDeviceRow` switches between `bg-zinc-950` and `bg-blue-600` for adjacent rows in the same list.
- `connectionConfig` badge colors define no `dark:` variant (unlike `trustConfig`, which does) — an online+connected device in dark mode likely renders its connection badge as a light pastel chip on a dark glass card. Could not verify live (no online paired device in the test environment); flagged as a code-derived risk.
- The deterministic detector's clean run is genuine, not broken — but its generic registry has no concept of "violates this project's own DESIGN.md," which is exactly the class of issue this page's biggest problems fall into. Worth remembering for future critiques of this codebase: a clean `detect.mjs` run here means "no generic cross-project anti-pattern," not "no design-system violation."
