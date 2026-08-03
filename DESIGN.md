---
name: SwarmDrop
description: The data channel between your devices — for humans and AI agents alike.
colors:
  harbor-teal: "oklch(0.583 0.105 177.1)"
  harbor-teal-dark: "oklch(0.641 0.115 177.6)"
  harbor-teal-foreground: "oklch(0.136 0.036 259.2)"
  brand-text: "oklch(0.516 0.093 178.2)"
  copper-core: "#C56A42"
  paper-white: "oklch(1 0 0)"
  graphite-ink: "oklch(0.145 0 0)"
  fog-secondary: "oklch(0.97 0 0)"
  fog-secondary-foreground: "oklch(0.205 0 0)"
  slate-muted-foreground: "oklch(0.556 0 0)"
  hairline-border: "oklch(0.922 0 0)"
  focus-ring: "oklch(0.708 0 0)"
  alert-red: "oklch(0.577 0.245 27.325)"
  app-shell-tint: "oklch(0.99 0.001 210)"
  glass-panel-surface: "rgb(255 255 255 / 0.58)"
  glass-control-surface: "rgb(255 255 255 / 0.64)"
  glass-card-rim: "rgb(255 255 255 / 0.34)"
  glass-control-rim: "rgb(15 23 42 / 0.055)"
  glass-brand-rim: "rgb(15 143 122 / 0.16)"
  aurora-mist: "#f7f7f7"
  aurora-cyan: "#22d3ee"
typography:
  headline:
    fontFamily: "ui-sans-serif, system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif"
    fontSize: "15px"
    fontWeight: 600
    lineHeight: "1.2"
    letterSpacing: "-0.01em"
  title:
    fontFamily: "ui-sans-serif, system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif"
    fontSize: "14px"
    fontWeight: 600
    lineHeight: "1.25"
  body:
    fontFamily: "ui-sans-serif, system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: "1.4"
  label:
    fontFamily: "ui-sans-serif, system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif"
    fontSize: "12px"
    fontWeight: 500
    lineHeight: "1.35"
  mono:
    fontFamily: "ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace"
    fontSize: "12px"
    fontWeight: 500
    lineHeight: "1.4"
    fontFeature: "tabular-nums"
rounded:
  sm: "6px"
  md: "8px"
  lg: "10px"
  xl: "14px"
  panel-sm: "18px"
  panel: "24px"
  full: "9999px"
spacing:
  xs: "4px"
  sm: "8px"
  md: "16px"
  lg: "24px"
components:
  button-primary:
    backgroundColor: "{colors.harbor-teal}"
    textColor: "{colors.harbor-teal-foreground}"
    rounded: "{rounded.md}"
    height: "36px"
    padding: "0 16px"
  button-primary-hover:
    backgroundColor: "{colors.harbor-teal}"
  button-outline:
    backgroundColor: "{colors.paper-white}"
    textColor: "{colors.graphite-ink}"
    rounded: "{rounded.md}"
    height: "36px"
  input-default:
    backgroundColor: "{colors.paper-white}"
    textColor: "{colors.graphite-ink}"
    rounded: "{rounded.md}"
    height: "36px"
    padding: "4px 12px"
  card-default:
    backgroundColor: "{colors.paper-white}"
    textColor: "{colors.graphite-ink}"
    rounded: "{rounded.xl}"
    padding: "24px"
  badge-default:
    backgroundColor: "{colors.harbor-teal}"
    textColor: "{colors.harbor-teal-foreground}"
    rounded: "{rounded.full}"
    padding: "2px 10px"
---

# Design System: SwarmDrop

## 1. Overview

**Creative North Star: "The Encrypted Workbench"**

SwarmDrop reads like a tool a developer would trust with their own files, not a consumer app trying to look friendly. The surface language is quiet by default — flat, restrained shadcn primitives (buttons, inputs, badges) with no bold color, no oversized type, no cheerful mascots — and it earns its personality from two deliberate layers on top: a **glass chrome system** that gives structural containers (panels, section shells, control chips) a soft, blurred depth without ever looking decorative, and a **WebGL ambient background** (a slow-drifting aurora, plus a teal/blue side-ray overlay in dark mode) that gives the whole app a sense of a live network breathing behind the UI. Every piece of machine-truth — peer IDs, pairing codes, transfer speeds, bootstrap addresses, MCP config snippets — renders in monospace with tabular numerals, never in the body sans. That's the tell that this is a workbench for people who read logs, not a lifestyle app.

This rejects the two anti-references named in PRODUCT.md directly: it is not a **generic SaaS dashboard** (no gradient hero, no identical card grid, no dashboard-shaped chrome for its own sake), and it is not a **heavy enterprise back-office** (no dense grey-on-grey tables, no Windows-panel density). Structure stays sparse: most screens are a single glass panel holding a short list or a focused action, not a grid of competing widgets.

**Key Characteristics:**
- Flat, quiet shadcn primitives at the control level (buttons, inputs, switches) — restraint over decoration.
- A separate glass-chrome vocabulary reserved for structural containers (panels, shells, control chips), never for buttons or inputs.
- A single accent color used sparingly; the WebGL ambient background carries most of the "alive" feeling, not saturated UI chrome.
- Monospace + tabular-nums for every literal machine value (IDs, hashes, codes, speeds) — the honesty tell of the system.
- Bespoke oversized radii (18–24px) reserved for panel-level chrome, standard 6–14px radii for interactive controls — two distinct radius vocabularies, not one scale reused everywhere.

## 2. Colors

The palette is almost monochrome by design — near-white/near-black neutrals doing most of the work — with a single teal action accent and a tiny copper brand core carried by the logo. The translucent glass layer borrows color from the app shell without becoming decorative.

### Primary
- **Harbor Teal** — light mode `oklch(0.583 0.105 177.1)` / `#0F8F7A`; dark mode `oklch(0.641 0.115 177.6)` / `#13A38C`. This is now the main color of the logo subject and the one saturated UI accent. Used for primary buttons, active/checked control states (switches, badges), focus selection, and send affordances.
- **Deep Ink** (`oklch(0.136 0.036 259.2)` / `#020817`): the foreground on every Harbor Teal fill. White text on the light teal fill is only about 4.0:1, so fills use this deep ink instead (about 5.0:1).
- **Brand Text Teal** (`--brand`, light `oklch(0.516 0.093 178.2)` / `#087968`, dark `oklch(0.828 0.12 179)` / `#5EE0C8`): the text-and-icon form of the accent. Raw Harbor Teal is tuned as a fill, so links, accent icons, and colored labels use `text-brand`, which clears 5.3:1 on white and 12:1+ on the dark canvas.
- **Copper Core** (`#C56A42`, dark lift `#E18762`): the small secondary brand color in the logo center. It adds warmth to the mark, but it is not a general UI action color.

### Neutral
- **Paper White** (`oklch(1 0 0)` / `#FFFFFF`): default light-mode background and card surface.
- **Graphite Ink** (`oklch(0.145 0 0)` / ≈ `#0A0A0A`): primary text color, light mode.
- **Fog Secondary** (`oklch(0.97 0 0)` / ≈ `#F5F5F5`): secondary/muted/accent surface fill — badges, hover states, subtle section dividers.
- **Slate Muted** (`oklch(0.556 0 0)` / ≈ `#737373`): secondary text — descriptions, timestamps, helper copy.
- **Hairline Border** (`oklch(0.922 0 0)` / ≈ `#E5E5E5`): the only border weight in the system; 1px, never thicker.
- **App Shell Tint** (`oklch(0.99 0.001 210)` / ≈ `#FBFCFC`): the outermost app background, a hair cooler than pure white so the ambient aurora reads through it.

### Semantic
- **Alert Red** (`oklch(0.577 0.245 27.325)` / ≈ `#E7000B`): destructive actions and error states only.

### Glass Chrome (signature layer)
- **Glass Panel Surface** (`rgb(255 255 255 / 0.58)`, blur 20px, saturate 145%): the background for full section shells (`SectionShell`).
- **Glass Control Surface** (`rgb(255 255 255 / 0.64)`, blur 12px): small chip-scale containers — icon badges, pairing-code cells.
- **Glass Brand Rim** (`rgb(15 143 122 / 0.16)`): the border on `glass-accent` surfaces — a translucent rim of Harbor Teal that marks emphasized glass areas (pairing code, active device card).
- **Aurora Mist / Aurora Cyan** (`#f7f7f7` / `#22d3ee`): the two colors driving the ambient WebGL background gradient; decorative only, never used in foreground UI.

Dark mode remaps every neutral (background → `oklch(0.18 0.01 260)` ≈ `#0F1216`, foreground → `oklch(0.965 0.002 260)` ≈ `#F3F3F5`, border → `oklch(0.39 0.01 260)` ≈ `#42454B`) and lifts Harbor Teal for glow against the near-black canvas. The cool navy-tinted dark neutrals deliberately stay: deep navy plus teal/copper keeps the product feeling secure without returning to the old heavy-blue identity.

### Named Rules
**The One Accent Rule.** Harbor Teal is the only saturated UI accent allowed outside the ambient background layer. Copper Core belongs to the logo and small brand moments only; if a screen needs a second "pop" of color, that's a sign it should be an icon or the WebGL background doing the work, not a second UI accent. (Connection-type badges — green LAN / sky hole-punch / amber relay — are semantic state coding, not decorative accents, and sit outside this rule.)

**The Brand Fidelity Rule.** ✅ Re-anchored after logo palette selection (2026-07): the brand primary is **Harbor Teal** (`#0F8F7A`) with a **Copper Core** (`#C56A42`) only inside the logo/small brand accents. The fixed anchors are `oklch(0.583 0.105 177.1)` (light fill), `oklch(0.641 0.115 177.6)` (dark fill), `oklch(0.516 0.093 178.2)` (light text form), and `oklch(0.828 0.12 179)` (dark text form). Contrast verified: Deep Ink on teal 5.0:1 (light) and 6.3:1 (dark), `text-brand` on white 5.3:1, on dark canvas 12:1+. **The two-token split (fill vs. text) is load-bearing: never use the fill teal as small text on white, and keep Copper Core out of ordinary UI state/action roles.**

## 3. Typography

**Body/UI Font:** system font stack — `ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto, sans-serif` (no custom webfont is loaded; this is a deliberate zero-font-loading choice, not an oversight, and matches "no telemetry, no external calls" restraint).
**Mono Font:** `ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace` — system monospace, same zero-loading logic.

**Character:** Quiet and small. This is a product surface with no hero copy and no display scale — the largest text in the entire system is a tabular-nums transfer-speed readout at `text-2xl`/`text-3xl` (24–30px), not a marketing headline. Hierarchy is carried by weight and color (foreground vs. muted-foreground), not by dramatic size jumps.

### Hierarchy
- **Headline** (600, 15px, tight tracking): `SectionHeader` titles — "设备"、"收件箱" style section titles atop a panel.
- **Title** (600, 14px): card/dialog titles, list-item primary labels.
- **Body** (400, 14px): default UI text, descriptions, dialog copy. Kept short — this is control copy, not prose, so the usual 65–75ch cap rarely comes into play.
- **Label** (500, 11–12px): counts, helper captions, badge text, muted secondary lines under a title.
- **Mono** (500, 11–13px, tabular-nums): every literal machine value — peer IDs, device fingerprints, pairing codes, bootstrap addresses, transfer speeds/sizes, MCP JSON snippets.

### Named Rules
**The Mono Truth Rule.** Anything that is a copy-able, verifiable, literal value — an ID, a hash, a code, a byte count — renders in monospace with tabular-nums. If a number or string can be checked against another system, it is never set in the body sans. This is the typographic expression of "state honestly, don't decorate."

## 4. Elevation

The system runs two elevation languages side by side, and the split is deliberate: **interactive controls stay flat**, **structural containers go glass**. shadcn primitives (button, input, card, badge) use only `shadow-xs` — a shadow so subtle it barely registers, there to keep controls from looking pasted onto the background, nothing more. Structural chrome — section shells, control chips, accent callouts — uses a dedicated glass system: `backdrop-filter: blur() saturate(145%)`, a soft ambient drop shadow, and an `inset 0 1px 0` highlight that reads as a light rim catching the edge of the glass. Depth in this system comes from blur and translucency, not from stacking dark shadows.

### Shadow Vocabulary
- **Control Flat** (`shadow-xs`, effectively `0 1px 2px rgb(0 0 0 / 0.05)`): default resting state for buttons, inputs, outline variants. Never intensifies on hover — hover changes background color, not shadow.
- **Glass Panel Shadow** (light: `0 18px 56px rgb(15 23 42 / 0.07)`; dark: `0 24px 84px rgb(0 0 0 / 0.28)`), paired with `inset 0 1px 0 var(--glass-highlight)`: the ambient shadow + rim-light combination for full section shells.
- **Glass Card Shadow** (light: `0 12px 32px rgb(15 23 42 / 0.045)`; dark: `0 16px 40px rgb(0 0 0 / 0.18)`): a lighter version of the above for nested glass cards/accents.

### Named Rules
**The Flat-Control, Glass-Chrome Rule.** If it's something you click or type into, it stays flat with `shadow-xs`. If it's something that holds other things (a panel, a shell, a chip wrapping an icon), it gets the glass treatment. Never apply `backdrop-filter` to a button or input; never leave a section shell shadow-less.

**The Reduced-Transparency Fallback.** Every glass surface has a `prefers-reduced-transparency: reduce` fallback that drops the blur and swaps to a flat `--card` background. Any new glass surface must ship this fallback in the same commit, not as a follow-up.

## 5. Components

### Buttons
- **Shape:** `rounded-md` (8px).
- **Primary:** Harbor Teal background, Deep Ink text, `h-9` (36px) default height, `hover:bg-primary/90` — the only hover treatment is a 10% opacity darken, no shadow or transform change.
- **Outline / Secondary / Ghost:** flat backgrounds (`background`, `secondary`, transparent respectively), same 8px radius and height family; outline is the only variant carrying `shadow-xs`.
- **Sizes:** a full scale from `xs` (24px) to `lg` (40px), plus matching icon-only squares — built for dense toolbar rows, not just one hero CTA per screen.

### Badges
- **Style:** `rounded-full`, 2px/10px padding, 12px text. Default variant uses Harbor Teal fill with Deep Ink text; outline/ghost variants exist for lower-emphasis tags (network status, transfer state).

### Cards / Panels
- **Corner Style:** two distinct radii by role — `14px` (`rounded-xl`) for shadcn `Card` (dialogs, settings tiles), `24px` for glass `SectionShell` (full-page panel chrome), `18px` for glass sub-panels (`EmptyPanel`, pairing-code cells).
- **Background:** `Card` uses flat `--card` (white/near-black); `SectionShell` uses `glass-panel` (translucent + blurred).
- **Shadow Strategy:** see Elevation — flat cards get `shadow-sm`, glass panels get the ambient glass shadow + inset rim.
- **Internal Padding:** `Card` uses 24px horizontal, 24px vertical sections; `SectionShell` uses a tighter 16px (`p-4`) since it's the outermost chrome, not inner content.

### Inputs / Fields
- **Style:** 1px hairline border, transparent/`bg-input/30` fill, `rounded-md`, `h-9`.
- **Focus:** border shifts to `ring` color plus a 3px `ring-ring/50` halo — no glow, no color change beyond the ring.
- **Error / Disabled:** invalid state adds a destructive-tinted ring; disabled drops to 50% opacity and disables pointer events.

### Device Card Contract (cross-platform)

**This section binds all three builds — desktop, mobile, and web.** They are three separate
implementations (React DOM / React Native / React DOM under a different bundler), so the shared
artifact is this spec, not a shared component. Data is not the constraint: the `Device` DTO all
three receive is generated from the same Rust struct (`crates/host/src/device.rs`) through three
codegens, so every field below is available everywhere.

**Required information slots.** A paired-device card SHALL present all eight:

| # | Slot | Source | Notes |
|---|---|---|---|
| 1 | Device-type icon | `os` / `platform` | |
| 2 | Display name | alias → peer `name` → hostname → short PeerId | via `@swarmdrop/shared-view` |
| 3 | Online state | `status` | **dot *and* word** — a bare colored dot does not satisfy this |
| 4 | Secondary identity line | groups + `hostname · shortPeerId` | only when names collide or groups exist |
| 5 | Trust badge | `trustLevel`, `trustConfirmed` | includes the "unconfirmed" state |
| 6 | Connection badge | `connection` + `latency` | latency shown whenever online and known |
| 7 | Send action | — | see Send Entry Contract |
| 8 | Overflow entry | — | unpair at minimum; trust policy and alias/groups where supported |

No build may drop a slot because "the layout is tight". Wrap, truncate, or move it to a secondary
disclosure — do not discard information the user needs to judge a transfer.

**Slot 6 disclosure — link details.** The connection badge SHALL be expandable into the link
evidence behind it: transport (`TCP` / `QUIC` / `WebRTC` / `WebRTC Direct`), the remote multiaddr,
and the relay's PeerId when one is in the path. That evidence is noise for an ordinary user and the
whole answer for anyone debugging, so it lives *behind* the badge, never beside it. The badge itself
may carry the transport name inline where the layout has room — it is one short token.

Shape is a permitted divergence: desktop and web use a popover (both have pointers, so a floating
layer costs nothing); mobile expands in place (an overlay on a touch device covers half the screen,
and the mobile surface is already a full detail page). What may not diverge: the transport name is a
proper noun and **is never translated** — users search it, paste it into issues, and diff it against
logs. And when the address carries no transport (an inbound relay connection's `send_back_addr` is
just `/p2p/<src>`), say "unknown" — do not invent a default.

The remote address must be copyable and must not be truncated in the disclosure. A truncated
multiaddr pasted into an issue is worthless.

**Degradation.** When a field is absent (offline device has no `connection`/`latency`), that badge
is simply not rendered; every other slot stays, and card height must not collapse or jitter. The
same applies one level down: `connectionDetails` is null until the kernel has reported a connection
address, and the badge then renders as a plain, non-interactive badge — an empty popover is worse
than no popover.

**Whole-card affordance.** A card body is clickable and does one of two things, consistently within
a build: trigger the single primary action (desktop: send), or open a device detail view (mobile).
If it opens a detail view, the send action MUST still be directly reachable on the card itself —
otherwise "sending starts from a device" quietly becomes "sending starts two taps from a device".
Nested controls (overflow menu, badges) stop propagation. When the card's action is unavailable
(device offline), the card is not clickable and visually degrades.

**Permitted divergence.** Each build chooses its own card orientation and grid, material (glass vs
flat), overflow container (dropdown menu / action sheet / inline second-step confirm / a device
detail screen), and confirmation shape (modal vs inline). None of these may remove a slot or change
what the primary action means.

**Known gap (desktop, tracked separately).** `src/routes/_app/devices/-components/device-card.tsx`
renders slots 5 and 6 as a ternary — the trust badge appears *only when* the connection badge does
not. Connected devices therefore never show their trust level, which is the one moment it matters
most. Mobile renders both. This predates the contract and is deliberately out of scope for the
change that introduced this section (`openspec/changes/web-ux-alignment` — its stated non-goal is
leaving desktop and mobile rendering untouched); fix it as its own change.

### Send Entry Contract (cross-platform)

- **Sending starts from a device.** Triggering send on a paired, online device card enters the send
  flow with that target already selected. The user never picks the same device twice.
- **The send page's target selector is a landing spot, not the main path.** It exists for deep links
  (`?peerId=`) and for correcting a wrong target — not as the way a user normally sends.
- **Offline devices do not offer send.** Disable or omit the action; never let someone click into a
  guaranteed failure and learn about it from a kernel error.
- **Deep-linked targets are untrusted input.** A `?peerId=` may point at a device that was unpaired
  or went offline since the link was made — say so in place and keep submit disabled.

### Cross-platform UI Review Checklist

Run this when adding or changing device-related UI in **any** build. It is a gate before visual
review, not after.

- [ ] All eight Device Card slots present (or explicitly absent because the data is)
- [ ] Online state shows a dot **and** a word
- [ ] Latency rendered whenever the device is online and `connection` is known
- [ ] Send reachable from the device card, target pre-selected
- [ ] Offline devices: send disabled, whole-card click disabled, visual degradation applied
- [ ] Destructive actions (unpair) require one explicit confirmation, and can report failure
- [ ] Display name / grouping / trust normalization come from `@swarmdrop/shared-view`, not a local copy
- [ ] Every interactive control reachable without hover; touch targets ≥ 44×44 CSS px
- [ ] New user-facing strings go through that build's i18n, including `aria-label` / `title` / `alt`

### Navigation — Desktop shell (Topbar + Breadcrumb)
- Desktop uses a single top bar: an unclickable logo mark, a node-status pill, a breadcrumb trail (home icon → intermediate clickable segments → unclickable current page), and window controls. There is no persistent sidebar in the current build — navigation depth is expressed through the breadcrumb, not through a nav rail.
- The topbar's only structural line is a 1px `rgb(255 255 255 / 0.34)` (light) / `rgb(255 255 255 / 0.08)` (dark) bottom hairline — no shadow, no background fill of its own beyond the ambient shell.

### Navigation — Web app area (Sidebar + Bottom Nav)
The browser build (`docs/app/app`, hosted inside the docs site) uses a **persistent sidebar**, not the breadcrumb-only topbar. This is a deliberate fork, decided in issue #88 — it is the "check against the current breadcrumb-only pattern" the Don't-list below asks for, and its conclusion applies to the Web area only: **the desktop shell does not change.**

Why the fork: the desktop app owns its entire window and can borrow the native title bar for chrome, so a breadcrumb is enough to say "you are inside an app". A browser tab has no such frame — the same tab renders marketing pages and docs — so persistent nav is the only structure that reads as an application rather than another document page. Deep-linkable routes also require a visible place to return to.

- **Sections** mirror the desktop information architecture exactly — devices / send / inbox / transfer / settings — so the two builds stay conceptually one product. Only the navigation *shape* differs.
- **Three responsive tiers:** ≥1024px expanded rail (icon + label, 224px) · 768–1023px icon rail (64px, label degrades to `title`/`aria-label`) · <768px fixed bottom nav with a sticky brand+status header. The single source of truth for the items is `docs/app/app/_lib/nav.ts`.
- **Active state** is `bg-fd-accent` + `text-[var(--brand)]` + `aria-current="page"` — the one-accent rule still holds; no second saturated color enters the chrome.
- **Count badges** (pending offers, in-flight transfers) use the brand solid fill with `--brand-ink` text. They exist because splitting one page into five routes hides time-sensitive inbound requests behind a route — the badge is the compensation for that, not decoration.
- **Node status stays visible in every tier** (pill when there's room, bare status dot in the icon rail): "state is honestly visible" does not get dropped because the window got narrow.

### Ambient WebGL Background (signature component)
A `Renderer`-driven (`ogl`) full-bleed canvas sits behind every app screen: a slow Perlin-noise "soft aurora" gradient (`aurora-mist` → `aurora-cyan`) always on, plus a teal/light-blue "side rays" overlay that appears only in dark mode. The loop is gated by `IntersectionObserver` + `visibilitychange` (pauses when off-screen or the tab is hidden) and fully respects `prefers-reduced-motion` by freezing on the first frame instead of skipping the effect outright — the texture stays, the motion doesn't. This is the system's single biggest personality investment; everything else in the UI stays deliberately quiet so this can carry the "alive network" feeling.

### Pairing Code Cell (signature component)
Individual pairing-code digits render as `glass-control` chips (`18px` radius, `font-mono text-3xl`, inset top highlight) rather than a plain OTP input row — the one place glass chrome and mono type meet directly, appropriate for the single most "trust me with a secret" moment in the product.

## 6. Do's and Don'ts

### Do:
- **Do** keep every interactive control (button, input, switch, badge) flat with `shadow-xs` only — glass/blur is reserved for structural chrome (`SectionShell`, `EmptyPanel`, control chips).
- **Do** render every literal machine value — peer ID, pairing code, hash, transfer speed, MCP config — in monospace with tabular-nums (The Mono Truth Rule).
- **Do** ship a `prefers-reduced-transparency` flat fallback and a `prefers-reduced-motion` freeze-frame fallback with any new glass or WebGL surface, in the same change.
- **Do** treat Harbor Teal as the only saturated UI accent; let the ambient WebGL background carry additional "alive" feeling instead of adding a second UI color.
- **Do** use the two-radius system deliberately: 6–14px for anything clickable, 18–24px for panel-level chrome.

### Don't:
- **Don't** build a generic SaaS dashboard: no gradient hero blocks, no identical repeated card grids, no dashboard chrome added for its own sake (PRODUCT.md anti-reference).
- **Don't** build a heavy enterprise back-office: no dense grey-on-grey tables, no Windows-style management-panel density (PRODUCT.md anti-reference).
- **Don't** apply `backdrop-filter` to a button, input, or any control someone clicks or types into — glass is for containers, not controls.
- **Don't** drop a Device Card Contract information slot because a layout got tight — wrap, truncate, or move it behind a disclosure, but don't leave the user unable to see how a device is connected or how much trust it has. (See "Device Card Contract"; it binds all three builds.)
- **Don't** add a second saturated accent color to the static UI chrome; if a screen feels flat, that's a signal to lean on the ambient background or an icon, not a new hex.
- **Don't** re-derive Harbor Teal from scratch or let Copper Core become a general UI accent — `oklch(0.583 0.105 177.1)` (light fill) / `oklch(0.641 0.115 177.6)` (dark fill) / `text-brand` for text form; treat these as fixed (The Brand Fidelity Rule). And never use the fill teal as small text on white — that's what `text-brand` exists for.
- **Don't** add a persistent sidebar nav rail to the **desktop shell** without checking against the current breadcrumb-only pattern first — it's a deliberate simplification, not an oversight. (The Web app area already went through that check and forked; see "Navigation — Web app area". A fork granted there is not a precedent for the desktop shell.)
