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

**Glass needs something behind it, and it needs the unprefixed property.** Two failure modes cost
the web build its glass for months, and both are invisible in a token diff:

1. `backdrop-filter: blur()` over a flat colour returns that same flat colour. Glass is only glass
   when the ambient layer is actually reaching it — which is why the aurora's mask must not hollow
   out the centre of the screen, where the panels are. Suppressing the ambient layer while keeping
   the glass classes buys the compositing cost and none of the effect.
2. The build can delete the standard property and keep only `-webkit-backdrop-filter`, which modern
   Chrome no longer honours. Same tokens, same classes, no blur. See the web knowledge base entry —
   the fix is to wrap the standard declaration in `@supports`, including the
   `prefers-reduced-transparency` reset.

**Panels are glass; rows inside them are solid.** Outer translucent, inner opaque is how depth is
stated here — a master-detail column and the list rows inside it must not both be glass (that is
the nested-card ban in another guise). Web keeps this as two constants, `PANEL_SURFACE` (glass) and
`ROW_SURFACE` (solid). Prior to 2026-08-06 both columns of inbox and transfer were solid, which
left those two routes with no glass at all while devices and settings had it — the same product
reading as two.

Dark mode remaps every neutral (background → `oklch(0.18 0.01 260)` ≈ `#0F1216`, foreground → `oklch(0.965 0.002 260)` ≈ `#F3F3F5`, border → `oklch(0.39 0.01 260)` ≈ `#42454B`) and lifts Harbor Teal for glow against the near-black canvas. The cool navy-tinted dark neutrals deliberately stay: deep navy plus teal/copper keeps the product feeling secure without returning to the old heavy-blue identity.

### Named Rules
**The One Accent Rule.** Harbor Teal is the only saturated UI accent allowed outside the ambient background layer. Copper Core belongs to the logo and small brand moments only; if a screen needs a second "pop" of color, that's a sign it should be an icon or the WebGL background doing the work, not a second UI accent. (Connection-type badges — green LAN / sky hole-punch / amber relay — are semantic state coding, not decorative accents, and sit outside this rule.)

**The Brand Fidelity Rule.** ✅ Re-anchored after logo palette selection (2026-07): the brand primary is **Harbor Teal** (`#0F8F7A`) with a **Copper Core** (`#C56A42`) only inside the logo/small brand accents. The fixed anchors are `oklch(0.583 0.105 177.1)` (light fill), `oklch(0.641 0.115 177.6)` (dark fill), `oklch(0.516 0.093 178.2)` (light text form), and `oklch(0.828 0.12 179)` (dark text form). Contrast verified: Deep Ink on teal 5.0:1 (light) and 6.3:1 (dark), `text-brand` on white 5.3:1, on dark canvas 12:1+. **The two-token split (fill vs. text) is load-bearing: never use the fill teal as small text on white, and keep Copper Core out of ordinary UI state/action roles.**

**The State Ink Rule.** Semantic state colors have a **fill form and a text form**, exactly like
the brand teal, and for the same reason: the fill values do not clear AA as small text
(measured on white: success 3.30:1, warning **2.13:1**). Dots, fills and icons use the base color;
**anything set as text uses the `-ink` variant** (`--success-ink` / `--warning-ink` /
`--destructive-ink`, 4.9–6.3:1 on the app surfaces). Mobile introduced this split; web adopted it
2026-08-04. A bare `text-amber-600` / `text-emerald-600` in a diff is the tell that someone reached
past the token.

⚠️ **Desktop only actually adopted it 2026-08-07.** This paragraph claimed all three builds had it
for three days while `src/index.css` carried no `--success` / `--warning` / `-ink` token at all —
so desktop had nothing to reach for and reached past by necessity: **159 palette literals** across
19 files (`text-green-600` for online, `bg-amber-100` for paused, `bg-zinc-950` for a selected
segment). They are now tokens, and the values are the same oklch expressions web uses. The lesson
is about this document, not the CSS: *"all three builds do X" is a claim to verify against the
files, not a plan recorded in the past tense.*

**A fourth state color: `--info` / `--info-ink` (sky).** The One Accent Rule below names sky as the
hole-punch connection color, but no build had a token for it — desktop wrote `sky-100/sky-600`, web
`sky-500/12`, and mobile gave up and reused `primary`, which made "hole-punch" teal on phones and
blue everywhere else *and* spent the brand color on a non-brand meaning. `--info` exists so the
three-color connection set (LAN `success` · hole-punch `info` · relay `warning`) is one row of
tokens rather than three dialects. It is **only** for that set; a new "informational blue" anywhere
else in the UI is a second accent and the One Accent Rule still forbids it.

### Cross-platform token unification

Three builds, three CSS dialects (desktop Tailwind v4 + oklch · web the same under fumadocs ·
mobile NativeWind, which needs raw HSL triples for `hsl(var(--x))`). **The values are one system;
only the notation differs.** Desktop and web write the *same oklch expressions verbatim* so the two
files can be diffed by eye; mobile carries converted HSL and a mapping note in `mobile/CLAUDE.md`.

What **must** match across all three — divergence here is a bug, not a fork:

| | Why |
|---|---|
| Brand fill / brand text / ink-on-fill | The Brand Fidelity Rule. Mobile shipped the *text* value as `--primary` (plus white ink) until 2026-08-04, which made every primary button a shade darker there than on desktop. |
| The `-ink` variants of success / warning / destructive | The State Ink Rule above. |
| Dark neutrals tinted **cool navy** (hue ~260) | Named below; the web build sat on fumadocs' pure-neutral `#121212` and mobile on a teal-blue hue 209 until 2026-08-04 — three products' worth of dark mode. |
| Card elevates *above* its surface | Light mode contrast between the two is inherently ~1.0:1; what carries elevation is direction + hairline + shadow. A card **darker** than its background reads as recessed and lands straight on the "grey on grey" anti-reference. |
| Focus ring uses the brand, not a neutral | A neutral ring measured 2.31:1 on the web surface — below SC 1.4.11's 3:1 and effectively invisible. |
| `--radius` scale and the two-radius vocabulary | 6–14px for controls, 18–24px for panels (§5). |

What **may** diverge, with the reason recorded next to it:

- **Dark focus ring on mobile** uses the bright text teal rather than the fill (10.5:1 vs 5.4:1 on
  that darker canvas). A focus ring's job is to be seen.
- **Light `--app-shell-background` is web-only tinted** — desktop `oklch(0.99 0.001 210)`, web
  `oklch(0.99 0.005 195)`. That layer is now the aurora's canvas on web; the extra chroma (toward
  the midpoint of brand teal 178 and aurora cyan ~207) is what lets the glass read as a layer in
  light mode. **Lightness stays 0.99 on both** — `--muted-foreground` sits directly on this layer
  and 0.985 measured 4.54:1, only 0.9% over AA. Do not "fix" the web value back to desktop's.
- **Two tokens exist on web only**: `--ambient-aurora-opacity` (0.7 dark / 0.48 light — the shader
  emits additive light, so a glow on near-black is a smudge on near-white) and `--glass-rail-bg`
  (the nav rail must be translucent or it clips the whole left edge of the ambient layer; desktop
  has no rail).
- ~~**The WebGL ambient background is desktop-only.**~~ **Overturned 2026-08-05.** The web build
  now ships it too; see "Ambient WebGL Background" in §5 for the shared contract and the web's
  five deltas. The original reasoning (`ogl` weight plus continuous GPU on a phone-browser
  baseline) was sound about *cost* and wrong about *what was being kept*: web dropped the
  background but **kept the glass layer**, and `backdrop-filter: blur()` over a flat fill returns
  that same flat fill. Measured: light `rgb(255 255 255 / 0.58)` over `oklch(0.99)` composites back
  to 0.99; dark `oklch(0.31 / 0.56)` over `oklch(0.145)` lands at ≈0.24 against a `--card` of 0.27.
  So the browser was paying for a compositing layer and receiving nothing — the same inbox
  empty-state code read as a finished product on desktop and as a skeleton screen on web.
- **Navigation shape** (desktop breadcrumb-only topbar vs. web sidebar) — see §5.

**Never** re-derive any of these by hand-converting one platform's value into another's notation
without writing the source expression down. The teal in web's `global.css` was hex for months and
*happened* to be the same color as desktop's oklch — nobody could tell without doing the math,
so nobody would have noticed it drifting.

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
| 6 | Connection badge | `connection` + `latency` | latency shown whenever online and known; transport name is **not** inline — see below |
| 7 | Send action | — | see Send Entry Contract |
| 8 | Overflow entry | — | unpair at minimum; trust policy and alias/groups where supported |

No build may drop a slot because "the layout is tight". Wrap, truncate, or move it to a secondary
disclosure — do not discard information the user needs to judge a transfer.

**Slot 6 vocabulary — four connection types, and `direct` is not `dcutr`** (2026-08-12). The badge
word comes from `ConnectionType` (`crates/host/src/device.rs`), which has four values: `lan` ·
`direct` · `dcutr` · `relay`. **The last two MUST NOT be collapsed into one word.** Both mean "no
byte goes through a relay", but they answer different questions and send a reader in opposite
directions: `dcutr` says NAT traversal succeeded — go look at ICE and signalling; `direct` says no
hole punching happened at all, the peer's address was simply dialable (a public IP, or a mesh-VPN
tunnel such as Tailscale).

**The distinction is the kernel's to make, never the UI's.** `PathKind` carries it directly
(`Local` / `Direct` / `HolePunched` / `Relayed`) and `path_to_connection` is a one-to-one map — no
build, and no layer above the kernel, may re-derive "was this hole punched?" from anything else.
In particular **do not infer it from the transport**: the native `dcutr` behaviour is enabled
(`presets.rs`, `Native`) and its successful punches come out as ordinary TCP/QUIC, so a
transport-based rule reports real hole punching as "direct". `crates/net-base`'s `PathKind` doc
records exactly which punches the kernel can currently name and which it cannot.

All three builds shipped the collapsed mapping until this date, which labelled a Tailscale
`…/quic-v1/webtransport` link "hole punched", sitting right above a disclosure row reading
`WebTransport`. `webrtc-direct` belongs to `direct`, not `dcutr`: it dials a bare IP with no
signalling and no traversal, which is the same distinction the transports clause below draws.

The two share one hue (`info`) and separate by icon and word — `direct` takes a bidirectional-arrow
glyph, `dcutr` keeps the bolt. A fourth hue would break the One Accent Rule, which opens its
exception to three.

**Slot 6 disclosure — link details.** The connection badge SHALL be expandable into the link
evidence behind it: transport (`TCP` / `QUIC` / `WebRTC` / `WebRTC Direct` / `WebTransport`),
the remote multiaddr,
and the relay's PeerId when one is in the path. That evidence is noise for an ordinary user and the
whole answer for anyone debugging, so it lives *behind* the badge, never beside it. The badge itself
may carry the transport name inline where the layout has room — it is one short token.

Shape is a permitted divergence: desktop and web use a popover (both have pointers, so a floating
layer costs nothing); mobile expands in place (an overlay on a touch device covers half the screen,
and the mobile surface is already a full detail page). What may not diverge: the transport name is a
proper noun and **is never translated** — users search it, paste it into issues, and diff it against
logs. And when the address carries no transport (an inbound relay connection's `send_back_addr` is
just `/p2p/<src>`), say "unknown" — do not invent a default.

**On a device card the badge carries connection type + latency and stops there** (2026-08-06; this
clause previously read "may carry the transport name inline where the layout has room"). Both DOM
builds tried the inline form on cards and neither has the room: `WebRTC Direct` is 13 characters,
wider than the icon, the type word and the latency put together, and device cards live in narrow
grid columns. What it pushed off the row was the trust badge and the send action — slot 5 and slot 7
losing their place to a detail of slot 6. Measured on web: 180px badge → 96px ("局域网 5ms").

So on the DOM builds the disclosure gained a middle step: **badge → hover summary → popover**, cost
rising with curiosity. The hover summary states the transport and says the popover holds the address.

**Mobile is already conformant and needs no hover substitute**: its cards never passed `transport`
to the badge. Its *device detail page* does keep the transport inline, and that is correct rather
than a fork — a detail page is not a grid cell, and its link-details block is collapsed by default,
so the badge is the only place the transport is visible without an expand. Room plus "the
alternative is hidden" is the test; a grid card fails both.

**The hover layer may never be the only route to anything.** The transport is still in the popover's
"transport" row, reachable by click and by keyboard — which is what the checklist item "every
interactive control reachable without hover" requires. A tooltip is a preview of a disclosure that
exists, never a disclosure of its own. Desktop pins both halves (`device-card.test.tsx`: the badge
does not contain the transport; a click reveals it) — pin them together or deleting the information
outright stays green.

**`WebRTC` and `WebRTC Direct` are two transports, not one abbreviated.** The first needs hole
punching and a signalling path; the second dials a public bare IP directly and is the browser's only
public entry point. Collapsing them reads as "swap the node and hole punching works too," which is
false. Any address-to-label helper MUST test `/webrtc-direct` **before** `/webrtc`, or the longer
one never matches — both builds' helpers had this ordering hazard, and desktop labelled
`webrtc-direct` as plain `WebRTC` until 2026-08-05.

**A truncated address must show where it was cut.** Both builds shorten bootstrap addresses for
display; dropping the middle without an ellipsis produces a string that still parses as a plausible
multiaddr (`/ip4/…/udp/4001/q/p2p/…` — the `quic-v1` segment silently gone), and users paste it into
issues believing it complete.

The remote address must be copyable and must not be truncated in the disclosure. A truncated
multiaddr pasted into an issue is worthless.

**Degradation.** When a field is absent (offline device has no `connection`/`latency`), that badge
is simply not rendered; every other slot stays, and card height must not collapse or jitter. The
same applies one level down: `connectionDetails` is null until the kernel has reported a connection
address, and the badge then renders as a plain, non-interactive badge — an empty popover is worse
than no popover.

**The send action shares a row with the badges; it never owns one** (2026-08-06). Slot 7 sits in the
card's footer row beside slots 5 and 6, at its natural width, right-aligned. Web shipped it as a
`w-full` button on a row of its own until this date: inside a ~300px card that is a full-bleed
saturated block, the loudest thing on the card, and all it says is "send" — the same failure the
Layout Density Contract names under "Full-bleed primary buttons are a landing-page move". The other
two builds were already right (desktop right-aligns in a `justify-between` footer, mobile uses a
44×44 icon button in that row), so this was web rejoining them, not a new rule. Below `sm:` the
button does go full width — that tier is thumb reach.

A corollary the web build learned the hard way: **a footer row only holds if slot 6 stays short.**
The full-width button was partly a consequence of the badge overflowing the row, not an independent
choice. Fix the length before adding a row.

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

**Trust normalization comes from the shared package.** `normalizeTrustLevel` and
`canSendToDevice` live in `@swarmdrop/shared-view`; no build re-derives them. The send affordance
is gated on `canSendToDevice`, not on `status === "online"` alone — an *online but blocked* device
must not offer send, and must not turn its whole card into a send target. (Desktop shipped the
online-only check until 2026-08-04; `device-card.test.tsx` now pins both.)

**Closed: the desktop trust/connection ternary.** Desktop used to render slots 5 and 6 as a
ternary — the trust badge appeared *only when* the connection badge did not, so connected devices
never showed their trust level, which is the one moment it matters most. All three builds now
render both. If a build ever needs to save space here, it wraps or truncates; it does not choose
one badge over the other.

### Incoming Request Contract (cross-platform)

**This section binds all three builds.** Two kinds of request arrive unsolicited — a pairing request
and an inbound file offer — and both demand a decision from someone who was doing something else.

**Global surface, not a page section.** Each build SHALL present these from a host mounted at the
app shell (desktop `_app.tsx`, mobile `app/_layout.tsx`, web `app/app/layout.tsx`), so they appear
on **every** route. A request rendered only inside one page is invisible to a user standing on any
other page — which, on a multi-route build, is most of the time. (This is exactly what the web build
did until the `lan-direct-upgrade` change: pairing requests lived inside the devices page, file
offers inside the inbox page.)

**Dismissal means opposite things for the two, and that is deliberate:**

| | Closing the surface | Why |
|---|---|---|
| Pairing request | **= decline** | The other side is blocked waiting on this answer. Leaving it pending gives them a spinner with no end. |
| File offer | **≠ decline** | The sender is queued, not blocked, and a mis-tap costing someone a whole transfer is far worse than one that costs a second click. |

Because a file offer survives dismissal, a build that lets it be dismissed **SHALL** also give a
place to find it again — both desktop and web put it at the top of the inbox list. A dismissible
request with no way back is a silently dropped transfer.

**The two clauses ship together.** Desktop shipped "close = decline" *and* no way back until
2026-08-04, with the close button and outside-click suppressed on top of it — the only exit was
Esc, which silently killed the sender's whole transfer. Whichever half you are changing, check the
other: "close ≠ decline" without a retrieval point loses the offer, and a retrieval point while
close still declines is dead code.

**Queue, don't stack.** Show one request at a time; the next appears after the current is resolved.
Say how many remain when more than one is waiting — all three builds do.

### Send Entry Contract (cross-platform)

- **Sending starts from a device.** Triggering send on a paired, online device card enters the send
  flow with that target already selected. The user never picks the same device twice.
- **The send page's target selector is a landing spot, not the main path.** It exists for deep links
  (`?peerId=`) and for correcting a wrong target — not as the way a user normally sends.
- **Offline devices do not offer send.** Disable or omit the action; never let someone click into a
  guaranteed failure and learn about it from a kernel error.
- **Deep-linked targets are untrusted input.** A `?peerId=` may point at a device that was unpaired
  or went offline since the link was made — say so in place and keep submit disabled.

### Receive Location Contract (cross-platform)

**Received files land where the user can find them — with the operating system's own tools, not
only ours.** This is a hard invariant, not a preference. It was violated for the entire life of the
mobile build, and the damage was not "slightly inconvenient": files landed in the app-private
directory, which no Android file manager can browse and no system picker can see. Users could
neither locate what they had received nor forward it to a third device, because every source in the
send flow is a system picker and not one of them can see a private directory.

- **No build may fall back to app-private storage for received files.** If no location is
  configured, receiving MUST be blocked and the user guided to configure one. A fallback that
  "always returns something" is exactly how the invariant was lost — `resolveReceiveLocation():
  string` could not express "not configured", so it returned the private directory.
- **The location is three states, not a string.** `ready` / `unconfigured` / `revoked`, each
  handled exhaustively. `unconfigured` and `revoked` MUST give different guidance: "pick a folder"
  versus "the folder you picked is gone, here is where it was".
- **App-private data never shares a directory with the receive area.** Database, staging, logs and
  any internal state live in a separate app-private location. Where a platform exposes a directory
  wholesale (iOS `UIFileSharingEnabled` exposes all of `Documents`), this is what decides whether
  the switch can be flipped at all.
- **Validate before accepting, never after.** A revoked grant surfaces as a silent write failure if
  checked late. The probe belongs in front of the accept action, and recovery MUST resume the
  interrupted accept rather than making the user start over.
- **"Show in folder" is available whenever the location is `ready`.** Never render an entry that is
  guaranteed to fail — and after this contract holds, that guarantee no longer exists on any build.

Permitted divergence — the platforms genuinely differ in what they offer:

| | Receive area | User choice |
|---|---|---|
| Desktop | User-chosen directory | Yes |
| iOS | `Documents`, exposed to the Files app | No — the system provides it |
| Android | User-chosen SAF tree | **Required**, asked during onboarding |
| Web | OPFS, with download as the delivery path | N/A — see below |

**Web is deliberately not the same shape.** OPFS is not "a filesystem the user can't see", it is
quota storage that the spec defines as invisible to the user; the browser's own idiom for handing
a file to a user is a download. The File System Access API is not a substitute: Safari and Firefox
support no directory picker in any version, so adopting it means either two sink implementations or
taking receiving away from those users. So the Web build treats OPFS as the holding area and
download as publication — the same two-stage model as `receive-staging-publish`.

### Received File Reuse Contract (cross-platform)

**A received file is a file. It can be sent onward.** Every build MUST offer this without routing
the user through the system share sheet — asking someone to "share a file with the app they are
currently using" is not a path, it is a workaround.

- **Reuse the file-first reverse flow** (files fixed → pick a device). It already exists, serving
  the system-share entry and "resend". The inbox is a third source, not a new concept — no new
  session type, no new IPC contract, no new transfer semantics.
- **The file source is the landing path itself.** Never copy, never derive it by joining save
  directory with relative path (under SAF that produces an unresolvable pseudo-URI, and a
  system-renamed `foo (1).jpg` breaks it outright).
- **Filter before initiating, not mid-transfer.** A file deleted from under us must be caught
  before prepare — hashing a dead URI fails the whole batch with an error that names nothing.
- **Two entry levels, no multi-select.** The whole record, and a single file. "Three of these
  seven" is a third level that needs selection state in `FileBrowser`, and it only covers the
  narrow gap between the other two.
- **This does not enter persistent navigation** and does not alter the Send Entry Contract's main
  path. Forwarding is an action in a file's context.

Shape may diverge with the input device: mobile pushes a screen (an overlay covers half a touch
screen), Web opens a dialog (pointers make floating layers cheap) — the same latitude the Node
Status Contract gives popover versus in-place disclosure.

**Desktop is exempt from providing the entry, and the reason is specific**: its receive location has
always been user-visible, so the system picker in the ordinary send flow already sees received
files, and "show in folder" gets the user there. The shared `FileBrowser`'s `onSend` action is
wired and available should that judgment change — desktop simply does not pass it today.

### Node Status Contract (cross-platform)

**This section binds all three builds.** Like the Device Card Contract, data is not the constraint:
all three receive the same `InfraLink[]` from `crates/core/src/infra/link.rs` through three codegens,
and the two judgments that turn it into a status word live in `@swarmdrop/shared-view`
(`deriveInfraLinkState`, `summarizeNodeHealth`). What each build writes is the rendering.

**Infrastructure is a role a relationship plays, not a category a node belongs to.** The same
`NodeId` may appear both as a paired device and as a relay — a LAN helper *is* another SwarmDrop
desktop. Builds MUST NOT model the two as mutually exclusive lists, and where they overlap the
device card carries an "also relaying for me" marker. Getting this wrong is not cosmetic: removing
an "infrastructure node" disconnects every connection to it, so on an overlapping node it kills a
running transfer — and an auto-discovered candidate is re-registered on the next identify, so the
button does nothing except break the transfer.

**Two layers of disclosure, not four.** A user opening node status is answering one of two
questions, and they belong to different people.

*Conclusion layer* (always visible — the pill, the sheet header, the settings summary):

| # | Slot | Source | Notes |
|---|---|---|---|
| 1 | Status dot **and word** | `summarizeNodeHealth().level` | a bare colored dot does not satisfy this |
| 2 | Reachability **consequence sentence** | `summarizeNodeHealth().msgId` | "cross-network devices can reach you" — never a subject-less adjective like "good" / "limited" / "reachable" |
| 3 | Paired N · online M | `NetworkStatus` + device list | clickable through to the devices page |
| 4 | At most **one** CTA | `summarizeNodeHealth().cta` | `null` is a valid answer; do not invent one for symmetry |

The two judgments return **msgIds, not copy** — each build renders them through its own catalog
(there are four: desktop, web, mobile, plus rust-i18n for the tray). The wording must match across
builds, so it is fixed here rather than in three places:

| msgId | Tone | CTA | 简体中文 | English |
|---|---|---|---|---|
| `nodeHealth.notRunning` | neutral | start node | 节点未运行 | Node is not running |
| `nodeHealth.starting` | neutral | — | 正在连接网络… | Connecting to the network… |
| `nodeHealth.reachable` | success | — | 其他网络的设备可以连到你 | Devices on other networks can reach you |
| `nodeHealth.lanReachable` | neutral | — | 只有同一网络里的设备能连到你 | Only devices on your network can reach you |
| `nodeHealth.configuredLanOnly` | neutral | open settings | 你关闭了公网可达性，其他网络的设备找不到你 | Public reachability is off, so devices on other networks can't find you |
| `nodeHealth.isolated` | warning | open diagnostics | 连不上任何网络，检查引导节点 | Can't reach any network — check your bootstrap nodes |
| `infraLink.seedOnly` | neutral | — | 仅 DHT 种子 | DHT seed only |
| `infraLink.excluded` | neutral | — | 已按设置排除 | Excluded by settings |
| `infraLink.settling` | neutral | — | 正在连接 | Connecting |
| `infraLink.ok` | success | — | 已就绪 | Ready |
| `infraLink.lost` | warning | — | 连接已断 | Connection lost |
| `infraLink.unreachable` | warning | — | 连不上 | Unreachable |

*Diagnostic layer* (one collapsed disclosure, default closed): every `InfraLink` as a row — status
word, attribution (source · scope · roles), and the **verbatim `lastError` with a copy button** —
plus local truth: node ID, reachable addresses, NAT, listen addresses, identity storage, uptime.

**Identity storage must be actionable, not a label.** On desktop the private key lives in an
owner-only file (`0600`), not the OS keychain, so this row gives the **copyable absolute path** —
backing up, migrating to a new machine and tightening permissions all start with finding the file.
A row reading "system keychain" or "a local file" tells the user nothing they can act on. Mobile
keeps a plain label (the OS secure store is not user-addressable); Web answers a different question
entirely ("am I still me after a refresh?") and is documented with its own copy.

Uptime belongs in the diagnostic layer, not the conclusion layer: it answers none of the four
questions above.

**"Some bootstrap nodes are down" is not a degradation.** Connecting to one of two relays has
exactly the same consequence as connecting to two. `1/2` is a diagnostic-layer fact; the persistent
slot MUST NOT warn on it, or users learn to ignore the status color.

**Alarm requires all three:** not caused by the user's own settings ∧ past the grace window ∧
actually blocking what the user is trying to do right now. `summarizeNodeHealth` only reaches its
warning level (`isolated`) when all three hold.

**Configuration is not failure.** A link excluded because the user turned off public reachability is
neutral-toned and its CTA is *open settings* — never *retry*. Same for a kad-only seed, which has no
relay track at all and therefore no failure state.

**Reachability warnings ride with the action, not the chrome.** `publicReachable == false` produces
no global banner. It becomes a blocking, in-place notice at the point it actually matters — invite
generation and the pairing entry ("this invite carries no address for you; a device on another
network can't use it").

**`lastError` is never translated.** It is what the user pastes into an issue and diffs against
logs; a translated string loses that use. It MUST be selectable/copyable — a long string that looks
clickable but isn't violates the copy affordance rule.

**No build may drop a slot because the layout is tight.** Collapse, scroll inside the sheet, or move
to the diagnostic layer — do not gate information on viewport height.

**Permitted divergence:** the surface shape. Desktop and web use a sheet/dialog; mobile uses a
bottom sheet. Desktop and mobile show NAT status and mDNS-discovered peer counts; **web omits those
two slots entirely** rather than rendering a permanently-`Unknown` field (see Degradation).
The listen-address slot is titled differently because it means different things: "Listen addresses"
on native (real sockets), "Reachable addresses" on web (circuit addresses that appear after a
reservation).

**Degradation.** A build renders a slot only where the value can be true. `nat_status` (autonat is
not compiled into the wasm target) and `discovered_peers` (no mDNS in a browser) are structurally
constant on web — omit the slot; a permanently-`Unknown` field is worse than its absence. `MdnsLanHelper`
sources and `lan` scope simply never occur there, so those groups render empty and need no special case.

**Network vocabulary is cross-platform.** The same concept gets the same word in every build. Three
catalogs had already drifted (`Bootstrap Nodes` / `Bootstrap nodes` / 「公网引导」/「引导节点」),
which is how the same screen ends up describing two things that are one thing:

| Concept | 简体中文 | English | Rejected spellings |
|---|---|---|---|
| A configured infrastructure peer (kad seed and/or relay) | 引导节点 | Bootstrap node | 公网引导 · 引导服务器 · Bootstrap Nodes (title case) |
| A LAN peer that relays for others | 局域网协助 | LAN helper | 本机 Helper · LAN Helper (mid-sentence caps) |
| The circuit path through an infrastructure peer | 中继 | Relay | 转发 · 中转 |
| A paired peer with a live connection | 已连接设备 | Connected device | 已连节点 |
| Others can open a connection to this machine | 可达 | Reachable | 在线 · 可访问 |
| Connection type — same local network | 局域网 | LAN | 本地 · 内网 |
| Connection type — the peer's address was dialable (public IP, or a mesh-VPN tunnel) | 直连 | Direct | 公网直连 · P2P · 点对点 |
| Connection type — NAT traversal succeeded | 打洞 | Hole-punched | 打洞直连 · Hole punching · **Direct** (collapses it into the row above) |

**The four connection types are one set and must stay four distinct words in every locale**
(2026-08-12). This is the clause the Slot 6 vocabulary rule leans on: splitting `direct` from
`dcutr` in the source language buys nothing if a translator collapses them again. Desktop's `en`
and `zh-TW` did exactly that — `打洞` was translated "Direct" / 「直連」, byte-identical to the new
`直连` row, so an English user saw one word for both a Tailscale tunnel and a real punch. Web said
"Hole-punched direct" and mobile said "Hole punching" at the same time, for the same badge.
Catalogs are independent by design (CLAUDE.md), so nothing mechanical enforces this — when you
touch one, diff all three.

Transport names (`TCP` / `QUIC` / `WebRTC` / `WebRTC Direct` / `WebTransport`) stay proper nouns
and are not translated — same rule as the Device Card Contract's slot 6. The list is whatever
`TransportKind` currently has; `@swarmdrop/shared-view`'s `transportLabel` is its single renderer.

### Transfer Progress Contract (cross-platform)

**A progress bar answers "how much of my transfer is done." Anything else wearing its shape is
lying.** Preparing a 1.99 GB file is about a minute of local hashing; on 2026-08-10 that minute
rendered as the same teal bar as the transfer itself, and was followed immediately by a second teal
bar starting at 0 on the detail screen. Users read the pair as one transfer that restarted. On a
*resumed* send it actually did start at 0, for an unrelated reason (second bullet) — two defects,
one indistinguishable symptom, which is what a shared visual primitive buys you.

- **Prepare and transfer MUST be visually distinguishable, with fixed semantics.** **Muted grey =
  this machine is preparing, nothing is on the wire yet. Brand teal = bytes are actually moving.**
  Recoloring the fill alone is not enough where the track is also branded (the shared `Progress`
  primitive tints its track `bg-primary/20`, so a grey fill lands in a teal trough) — the whole
  primitive changes tone, or it has not changed.
- **The resume baseline comes from the fetch plan, never from `transferred_bytes`.** That column is
  written only on graceful paths, so an Android process kill leaves it at 0 — and a resume that
  correctly re-sends only the missing 4206 of 8136 chunks still draws from 0% and "finishes" at
  51.7%. The receive side has always been right and is the model to copy: it recounts from the
  bitmap it checkpoints every ten blocks, so it survives a kill. The fix lives in shared code, so
  it lands on **desktop and mobile** together — those are the two builds that can resume a send at
  all. **The web build is out of scope by construction, not by omission**: a browser cannot re-read
  the same `File` unless the user re-picks it, so non-terminal send sessions and pending offers are
  never persisted there and no send is ever resumed. Web resumes receives only, and that path was
  already bitmap-derived.
- **A resumed transfer MUST state its baseline in place** — a tick on the bar plus "resumed ·
  continuing from X". Silently starting at 63% confuses as much as starting at 0, and the baseline
  is the only thing that lets a user tell correct resume behavior from the two failures above.
  Expect the honest number to sometimes step *backward* from a graceful pause (the local column can
  be a few blocks ahead of the peer's checkpoint); show the truth.
- **Any local phase over ~3 s carries a percentage or an ETA.** Prepare, outboard rebuild after an
  invalidated cache, the Android SAF publish copy (a full copy — a 6 GB file writes 12 GB), and the
  web build's directory retrieval (`docs/app/app/_lib/zip-download.ts` reads every entry out of OPFS
  and materializes one Blob) all cross that line. A phase that shows only a spinner for a minute is
  indistinguishable from a hang, and the user's answer to a hang is to force-quit the app — which is
  precisely what produces the zeroed `transferred_bytes` in the second bullet.

**An active transfer has four slots, and speed is the least useful of them.** "12.4 MB/s" makes the
user do the division — *bytes left ÷ that number* — to reach the only question they actually have.
Every mainstream transfer UI shows the answer instead. The data has always been there: `eta` has
been on `TransferProgressEvent` since it was written, and all three builds carry it into their
stores; what was missing was rendering.

| # | Slot | Source | Notes |
|---|---|---|---|
| 1 | Percent | `calcPercent(transferred, total)` | clamped to 100; the resume baseline rule above governs where it starts |
| 2 | Bytes done / total | `formatFileSize` ×2 | the first slot to drop on a secondary surface |
| 3 | Speed | `formatTransferRate` → `—` when null | rate, not a promise; see below |
| 4 | **Time remaining** | `formatEta(progress.eta)` | `null` means *cannot compute*, which is **not** the same as `0s` |

- **Slot 4 appears only while a transfer is `active`.** Completed, failed, paused and
  waiting-for-acceptance builds MUST NOT show it — a paused transfer has no speed, so a remaining
  time there reports a wait that isn't happening.
- **The primary surface carries all four. A secondary surface (list row, card, notification) carries
  at least 1 and 4, and where only one of speed/ETA fits, ETA wins.** Speed is the slot to drop on a
  narrow row, not the answer.
- **When ETA cannot be computed, show a placeholder — never let the slot vanish.** Speed already
  degrades to `—`; a slot that disappears instead reads as a layout bug, and it disappears exactly
  when the transfer is in trouble. `formatEta` returns `null` rather than baking the copy in, so the
  placeholder is the call site's translated string.
- **ETA is quantised for display, never smoothed with state.** `formatEta` rounds up to 5 s / 10 s
  steps. Stateful smoothing would have to live either in three renderers (which drift apart) or in
  `ProgressTracker` as a field that exists only for display.
- **A stalled transfer must not keep showing its last good number — and that takes work on both
  sides.** `ProgressTracker::speed()` returns `0.0` once the newest sample is older than the sliding
  window, so the *next* frame after a stall carries `speed: 0` and `eta: null`. But there may be no
  next frame: progress events are emitted from the block-receive path, and nothing ticks on its own,
  so a peer that goes quiet leaves the last frame sitting in the store forever. **Each build
  therefore ages its own copy**: record when the last progress frame arrived, and treat `eta` as
  unavailable once it is older than `PROGRESS_STALE_MS` (`@swarmdrop/shared-view`, 6 s = 2× the
  sliding window). Slot 4 falls back to its placeholder. Without this the ETA does not disappear
  when the transfer is in trouble — it *lies*, which is worse.

The four strings this contract adds, fixed here because they must match across the four catalogs
(desktop · web · mobile Lingui; rust-i18n does not render transfer progress):

| msgId (source locale `zh`) | 简体中文 | 繁體中文 | English |
|---|---|---|---|
| `剩余 {0}` | 剩余 {0} | 剩餘 {0} | {0} remaining |
| `计算中` | 计算中 | 計算中 | Calculating |
| `正在保存 {0}` | 正在保存 {0} | 正在儲存 {0} | Saving {0} |
| `正在保存…` | 正在保存… | 正在儲存… | Saving… |

**"Bytes received" is not "file saved", and on Android the gap is minutes.** Receiving is staging →
publish; the last progress frame fires at 100% *before* publish starts. Desktop, web and iOS publish
in O(1) (same-volume rename, OPFS close), but an Android SAF target is a full byte copy. A satisfied
progress bar followed by a silent wait is the exact shape users read as a hang — and force-quitting
is what produces the zeroed `transferred_bytes` in bullet 2. All three builds therefore render a
publish state in `tone="local"`. **The primary surface names the file (`正在保存 {0}`); secondary
surfaces may use the bare `正在保存…`** — the row is already showing which file it is, and the name
is what gets truncated first on a narrow row anyway. Android carries a percentage in it, taken from
the `written` counter its copy loop was already keeping; a constant-time rename has no loop and
needs none. The state begins when publish is entered, not when the first byte is written — building
the target directory tree is part of the wait.

The event carrying this is **file-level, not session-level**: publish happens per file, the moment
that file completes, so a hundred-file session publishes a hundred times mid-transfer. It is also
purely local — it never crosses the wire.

Two consequences of it being file-level, both binding:

- **The publish state is a swap inside the active layout, not a layout of its own.** A build that
  swings to a different block for it makes a hundred-file session restructure a hundred times, and
  on mobile it also silently changes what the big number means (session percent → this file's copy
  percent), so the reader watches progress drop from 87% to 4% and back. Keep the session's percent
  and its bar; change the tone and the one slot that would otherwise be claiming a network rate.
- **Publish under `PUBLISH_VISIBLE_AFTER_MS` (`@swarmdrop/shared-view`, 300 ms) is not shown at
  all.** On the three O(1) builds `started` and `finished` arrive back-to-back but as two separate
  events, so rendering them eagerly strobes the bar grey once per file — exactly the "visual jolt
  reads as a restart" failure this contract opens with, at a hundred times the rate. Delay the
  reveal; the phases that actually need explaining are all far longer than the threshold.

**Progress must cover the real work, not the part that was easy to instrument.** Prepare reads the
source exactly once and the progress events ride that same pass; a bar that completes and then
leaves the user waiting the same duration again is worse than no bar.

**Open gaps as of 2026-08-10 — this contract is not yet fully met, and saying so is part of the
contract.** The Node Status Contract already cost us once by reading as a description of the code
when it was really a wish list, so: bullet 1 landed on all three builds that day (tone split plus a
percentage in the prepare copy), and bullet 2 landed in the shared resume planner
(`crates/transfer/src/flow/resume/plan.rs`) — which is to say on the two builds that can resume a
send at all. Bullet 3 has **no implementation anywhere** — nothing renders a baseline tick or a
"resumed · continuing from X" label yet. Bullet 4 holds for prepare and, since 2026-08-10, for the
Android SAF publish copy; **outboard rebuild** and the **web build's zip retrieval** (a bare
`LoaderCircle` plus "下载中…", `docs/app/app/_components/inbox-views.tsx`) still show a spinner with
no progress. These are open bugs, not permitted divergence — delete these sentences in the PR that
closes them.

**Permitted divergence: none — but no single diff covers all three.** The prepare bar exists three
times (`mobile/src/components/transfer/prepare-progress-bar.tsx`,
`src/routes/_app/send/-components/prepare-progress-bar.tsx`,
`docs/app/app/_components/prepare-progress.tsx`) over three different progress primitives, and the
primitives need different work to express the same tone split: mobile's track is already neutral so
only the fill moves, desktop's track is branded so fill *and* track move, web's is a single shared
component. That is a difference in **what each patch touches**, not in what any of them may expose.

**The one criterion, binding on all three builds:** where a progress primitive draws more than one
phase — and all three do — **the phase is a named prop resolved by a lookup table inside the
primitive**; call sites pass the phase, never raw color classes. The established prop name is
`tone`, with a `Record<ProgressTone, …>` beside it. Adding a fourth phase is then a compile error
at one place instead of a color someone forgot at one of N call sites, and the "is grey allowed
here?" question stops being re-litigated per screen. **Copy the rule, not the patch.**

There is a **fourth** primitive the paragraph above misses: `packages/file-browser/src/progress.tsx`,
shared by desktop and web for per-file rows. It carries a different pair — `transfer` vs `paused` —
which is *orthogonal* to `transfer` vs `local`: one splits "preparing locally" from "actually on the
wire", the other splits "moving" from "stopped". A primitive takes only the axis it draws. The rule
above still binds: the lookup table converted two call sites that had been passing raw
`bg-warning`/`bg-primary` ternaries, and it did so by failing to compile.

**Paused is amber, and amber means the `-ink` variant.** A paused transfer is a *user-recoverable*
interruption — it has to be scannable in a list of files, so it does not get the neutral grey that
"waiting" gets. But the raw `--warning` is a light amber that only clears **2.14:1** against a light
card and **1.83:1** against its own 20% track: below the 3:1 that WCAG 2.2 SC 1.4.11 requires of
non-text. Fill and icon therefore use `--warning-ink` (4.33–5.05:1 light, 5.45–8.90:1 dark). This is
the same rule `src/index.css` already states for status colors as text — measured 2026-08-10 after
mobile shipped raw amber on both the fill and the icon. **Dark mode passes either way**, so checking
only the dark theme will tell you nothing.

### Transfer List Order Contract (cross-platform)

**The transfer list is one timeline, ordered by last activity. It is not grouped by state.**

Until 2026-08-12 all three builds sorted by state first and time second, each with its own shape:
desktop ranked active → suspended → everything else and then sorted by `startedAt`; mobile rendered a
four-section `SectionList` (active / recoverable / attention / completed); web split into `active`
and `history` with a heading between them. The three also disagreed on the key — desktop used
`startedAt`, the other two `updatedAt` — so **the same resumed session sat at the top on two builds
and near the bottom on the third**.

- **The key is `updatedAt` — last activity, never `startedAt`.** The question this list answers is
  "what happened recently". A session started three days ago and resumed a minute ago is the single
  most relevant row on the screen, and sorting by start time buries it three days down. Pause-then-
  resume-tomorrow is routine, not an edge case.
- **A live transfer is recognised by being the only row that moves, not by its position.** The DB
  does rewrite `updated_at` on every checkpoint (`update_file_checkpoint_ranges`), but that fact
  never reaches a list: checkpoints emit `TransferProgress`, while `TransferProjection` is emitted
  only on a state transition (`crates/transfer/src/coordinator.rs`), and no store writes `updatedAt`
  from a progress event. **So a 20-minute transfer has its `updatedAt` frozen at the moment it went
  `active`, and any session that finishes meanwhile sorts above it.** That is the accepted trade of
  this contract, not an oversight — the active row carries the only progress bar and the only
  animation on screen, and the "active" filter is one tap away. If it ever stops being acceptable,
  the honest fix is a throttled projection emit on checkpoint, *not* a state tier bolted back on top
  of the ordering.
- **Ties break on `sessionId`.** The list source is `Object.values(projections)`; without a
  deterministic tiebreak two same-millisecond rows swap places on re-render for no visible reason.
- **Grouping is the user's move, not the list's** — which obliges every build to keep a filter for
  each class its grouping used to surface. Flattening mobile without adding a `可恢复` chip would
  have deleted the only affordance for "which one can I resume": its `attention` predicate returns
  `!recoverable` for a suspended session, so recoverable was the one class its filters could not
  reach. Baking that split into the default view charges every reader for it: a
  just-failed session renders *below* a pile of days-old completions because it lives in a later
  section. Mobile's search screen had already reached this conclusion independently — "搜索场景没有
  分组语境,卡片保留状态徽章承担状态信息".
- **Consequence, binding: every row states its own state.** With no section heading above it, the
  status badge is no longer redundant and MUST NOT be suppressed. Mobile previously hid the badge
  inside the *active* and *completed* sections precisely because the heading said it; flattening
  without re-enabling the badge is how "已完成" and "失败" become indistinguishable.

**The storage ports are not bound by this contract.** `list_transfer_projections` returns
`started_at` descending on both implementations (`crates/storage-sql`, `crates/web`); that is a
*determinism* guarantee, not a presentation order — the front-end Record is rebuilt from incremental
events anyway, so the port order is gone by the first update. `started_at` is the better anchor there
precisely because it never changes.

**One implementation, not three:** `sortByTimelineDesc` / `compareByTimelineDesc` in
`@swarmdrop/shared-view`. The comparator takes `{ sessionId, updatedAt }` and compares rather than
subtracts, because **mobile's `updatedAt` is a `bigint`** (uniffi i64) and `bigint - number` throws.
Anything re-deriving this order locally is the drift this contract exists to prevent.

- **Every row prints the timestamp it was sorted by.** Printing `startedAt` while sorting on
  `updatedAt` puts "3 天前" at the top of the list on the contract's own headline scenario. Terminal
  sessions are unaffected in substance — their `updatedAt` *is* the moment they ended.

**Permitted divergence: the list primitive and the filter sets.** Desktop and web render into a
master-detail shell, mobile into a `FlatList`. Mobile's filters are `all / receive / send /
recoverable / attention` — direction-first, because that is the question a phone user arrives with;
the other two are `all / active / recoverable / ended`. Neither licenses a second ordering rule.

**Open gaps as of 2026-08-12 — this contract is not yet fully met, and saying so is part of the
contract** (same discipline as the Transfer Progress Contract above; the Node Status Contract cost
us once by reading as a description of code that was really a wish list):

- **Desktop and web share four filter names but not four predicates.** Desktop's `active` is
  offered/waiting/active and its `ended` also absorbs *non-recoverable* suspended sessions; web's
  are `phase !== "terminal"` and `phase === "terminal"`. A non-recoverable interruption therefore
  files under 已结束 on desktop and 进行中 on web. This is the same shape of drift as the
  `startedAt`/`updatedAt` split this contract just closed, one chip to the left. Closing it means
  deciding which reading is right — a real product call, not a move — so it is deliberately left
  out of the PR that wrote this contract. The three predicates belong in
  `@swarmdrop/shared-view/transfer` beside the comparator; the filter *sets* stay per build.
- **Web draws one grey dot for every terminal session** (`PHASE_META[phase].dot`). While 已结束 was
  its own section that was fine; interleaved into a timeline the dot becomes the only
  pre-attentive signal and it says nothing — completed and failed look identical until you read the
  label. Desktop and mobile already colour by outcome.
- **Mobile lost one line of grouping knowledge** with no new home: the 已完成 section's subtitle
  「收到的内容已放进收件箱」. The per-row inbox button is its actionable form and the list header
  still says it once, so this is an accepted loss rather than an open bug — recorded because it is
  the only thing in that flattening that genuinely belonged to the group layer.

### Bottom Action Contract (mobile)

**A pinned bottom action is not trim; when it fails, a feature disappears.** The device detail screen
shipped with no scroll container and the wrong bottom-bar primitive, which pushed its policy entry —
the only route in the entire mobile build to unpair, block a device, or change its receive policy —
permanently past the screen edge once "connection details" was expanded. It survived review because
the collapsed state happens to fit.

1. **Stack and detail screens use `BottomActionBar`, always** (`mobile/src/components/mobile/screen.tsx`),
   placed in `AppScreen`'s **children** — never in the `footer` slot. It is what guarantees the three
   things a pinned bar needs: the top hairline, an **opaque** `bg-background` (content MUST NOT show
   through; a transparent bar over a scrolling list reads as a rendering bug), and bottom padding
   that clears the system inset. The `footer` slot belongs to tab screens, where the opaque native
   tab bar underneath already supplies background and inset.
2. **Bottom distance is inset *plus* breathing room — added, not `Math.max`.** The inset is space the
   *system* occupies; breathing room is visual whitespace. Every current device's inset already
   exceeds any sane breathing value (Android gesture bar 24dp, three-button 48dp, iOS home indicator
   34dp), so `Math.max(inset, 12)` collapses to `inset` on all of them and the primary button sits
   flush against the system bar everywhere.
3. **A screen with a pinned bottom bar MUST make its content a scroll container** (`AppScreen scroll`,
   or an explicit `ScrollView` / `FlatList` / `FlashList`). The bar is an in-flow sibling and is
   **never absolutely positioned**, so the content side does NOT add padding equal to the bar's
   height — only a breathing gap at the end. Doing both is the other way this goes wrong: a dead band
   the user can scroll into for nothing.
4. **`flex-1` is judged by the *parent's* `flexDirection`, never by whether the node itself is a row
   or a column.** `BottomActionBar` is `flex-row`, so every **direct** child of it — column wrappers
   included — needs `flex-1` to fill the bar's width; a direct child without it shrinks to its text
   width and the whole bar reads as a stray label. The failure mode is `flex-1` appearing where the
   **parent is a column**: an inner vertical stack inside the bar, or a control inside such a stack.
   There `flexBasis: 0%` resolves against **height** with an auto-height parent, the node collapses,
   and content overflows the bar symmetrically (a progress bar riding above the hairline, a button
   clipped by the screen edge) because the row centers on its cross axis.

   The two send screens are the worked example, and the shape is deliberate — do not "simplify" the
   wrapper away (`mobile/src/app/send/select-device.tsx`, `share-target.tsx`):

   ```jsx
   <BottomActionBar>                 {/* flex-row */}
     <View className="flex-1 gap-2"> {/* column, but its parent is the row ⇒ flex-1 = width. Required. */}
       {prepareProgress ? <PrepareProgressBar /> : null}
       <Pressable … />               {/* parent is the column ⇒ no flex-1 here */}
     </View>
   </BottomActionBar>
   ```

   Where the bar's single direct child simply *is* the primary button, that button is the one
   carrying `flex-1` — same rule, shorter tree. Being a column is not what makes a node wide; being
   a child of a row with `flex-1` is.

**Verify across three Android navigation modes and two iOS shapes.** Gesture bar (24dp),
three-button (48dp) and fullscreen (0dp) differ exactly where rule 2 bites — the first two show it,
the third cannot. iPhone X+ (34dp) versus iPad/SE (0dp) likewise. A bottom bar that looks right on
one simulator proves nothing. Where the bar rides a keyboard (`KeyboardStickyView`), check both
keyboard states: the system inset is redundant while the keyboard is up.

**Three-button is not "the same bug, 24dp worse" — it is a different bug.** The gesture bar only
consumes swipes, so a control sitting under it stays tappable and the defect degrades to a mistap
risk. The three-button bar consumes **taps**, and they are Back / Home / Recents: an overlapped
control loses that height outright, and the user aiming at it gets ejected from the app instead.
Any bottom-inset shortfall must therefore be scored at 48dp, not 24dp — a 44dp control with a 16dp
gap keeps 36dp under the gesture bar but only 12dp under three-button, well under the 48dp minimum
touch target. Scoring one archived audit at 24dp alone is how a reachability defect got filed as
polish (`dev-notes/research/2026-08-10-mobile-bottom-action-audit.md`, S1-b).

**One primitive, not two.** A second, nearly identical bottom-bar component with no background and
no inset is how the device detail bug happened — the author picked the wrong one from an
autocomplete list. Where a variant has a single legitimate call site, inline it there rather than
exporting it beside the real one.

### Layout Density Contract

Written from the web build's 2026-08 rework, but the failure it names is not web-specific — check
any build against it.

**Spacing encodes grouping, so the steps must differ.** The web app area shipped for months with
three identical 16px values: section-to-section gap, in-panel gap, and panel padding. When those
collapse to one number, Gestalt proximity carries no information and grouping can only be stated by
drawing boxes — which is exactly what the pages looked like, a stack of equally sized containers.
The ratio now is **8 : 16 : 32** (`--space-in-group` / `--space-in-panel` / `--space-section`), with
panel padding on its own step at 20px (`--space-panel`) because container padding follows the
container's *role*, not its contents.

- Between-group distance MUST be visibly greater than within-group distance.
- Panel padding MUST NOT equal the panel's internal gap, or the panel stops reading as a container.
- Verify by blur test: blur the screen until text is illegible; grouping must still be readable.
- Reach for spacing before a divider. A hairline that exists because two blocks sit 16px apart is
  spacing that gave up.

**Type steps must be distinguishable.** Page title was 16px against a 15px section title — a 1px
step is not a hierarchy, it just puts the burden on position. The ladder is **20 / 15 / 14 / 12**
(page title with `-0.02em` tracking · section headline · body and list-item titles · labels), plus
mono at 11–13px for machine values (The Mono Truth Rule is unchanged).

**Column width is one number site-wide; line length belongs to the text, not the page.**
(Revised 2026-08-06 — this clause previously specified three tiers: 1240 boards / 1040 settings /
860 forms.) Tiering by content type was the right instinct, but all three were centred with
`mx-auto`, so the content's left edge **jumped between routes**: measured at a 1440 viewport,
devices/inbox/transfer sat at 224, settings at 307, send at 402 — up to 178px of travel between
entries that sit next to each other in the rail. A stable left edge does more for "this is an
application" than each page's ideal measure does.

So the page gives one width (1240, same as the desktop `master-detail-shell.tsx`) and **the text
constrains itself**: a form panel that should stay narrow sets its own `max-w`. That is the correct
layer anyway — measure is a typographic property, and binding it to the page container makes a
paragraph's readability depend on which route it happens to live on.

One rule survives from the old clause, and it is the reason tiering was tried in the first place:
**a panel that constrains itself must not centre itself.** Full-width header above a centred panel
misaligns the two left edges. Constrain left-aligned instead.

**Empty states size to their role.** An empty state that IS the whole column (a master-detail
detail pane) fills and centers. An empty state that is one section *inside* a panel must not — a
filled one leaves a ~320px cavity where content belongs, and three of those on a page is the
"stack of equal empty boxes" look again. Sections that are usually empty (in-flight transfers)
should collapse to a single row rather than render a titled panel around a void — but they may not
disappear, because the entry point they carry has to survive.

**Full-bleed primary buttons are a landing-page move.** A form's primary action belongs in a
right-aligned footer at its natural width (full width below `sm:`, where thumb reach wins). A
1100px saturated fill, usually rendered disabled, is the loudest thing on the page and says nothing.

**Settings runs its own primitives, and that split is deliberate.** Desktop and web both build
settings from `SettingsSection → SettingsCard → SettingsRow` (`src/routes/_app/settings/-settings-primitives.tsx`
and `docs/app/app/_components/settings-primitives.tsx` — two implementations, one standard) rather
than the page-panel primitive the rest of the app uses. The reason is content shape: list-and-grid
pages need a container with presence, while settings is a run of small labelled rows whose grouping
comes from hairlines inside one card. Wrapping every settings group in a page-level panel produces
the stack of oversized cards both builds started with. Section titles drop a step to match (14px
semibold + a bare brand icon, not the panel primitive's icon chip), and the bento grid tiers
(`md:grid-cols-2` / `lg:grid-cols-6` with spans, 1040px column) are shared.

Desktop pairs cards by height to keep row bottoms level; that only works when a column has enough
sections to stack. Where it doesn't — web's preferences row is one short card beside one tall one —
size to content instead. Stretching leaves a third of a card as empty glass, which reads as
unfinished, not as breathing room. **Copy the rule, not the conclusion.**

Desktop's own devices page violated this until 2026-08-06 and much more severely: the right-hand
"add device" panel was `flex-1`, so with 16 paired devices it stretched to 1468px around 278px of
content — **1206px of empty glass, 82% of the card**. The tell that it is the wrong lever: the
emptiness scales with *the other column's* content.

**A short aside column beside a long list should follow the scroll.** Once it sizes to content, the
entry points in it (pairing, and a *live-updating* nearby-device list) scroll out of view while the
user is still working through the list — and content that changes on its own is worthless where it
cannot be seen. Make it `sticky` with `self-start`; a stretched grid item has no slack to stick with
and the rule silently does nothing. Cap the one section inside it that grows without bound (nearby
devices) rather than the card — a sticky element taller than the viewport puts its own bottom
permanently out of reach, and capping the card instead would clip its glass corners.

**The devices page splits at 1280px, and that is a second breakpoint on purpose.** 920 measures
*master-detail* (list ↔ detail, both panes are content). The web devices page is a different
shape — main content plus one column of auxiliary tooling (pairing) — and the web app area carries
a navigation rail of its own, which 920 does not account for. The rail has three tiers
(≥1024 expanded 224px · 768–1023 icon 64px · <768 bottom nav), so **the same viewport width leaves
different content width on desktop and web**: the desktop devices page can split at 920 precisely
because it has no rail.

At 1280: `1280 − 224 (rail) − 48 (page padding) = 1008` content, minus a 360 pairing column and a
32 gutter leaves a 616px main column — exactly two device cards (280×2 + 8). One tier down (1024
viewport) the main column is 376px, which fits one card; a single-card main column beside a 360px
sidebar reads as two things side by side rather than one main and one aside.

Below 1280 the page stacks in the previous order (grid → active transfers → pairing), and pairing
collapses to a single header row. **The CSS `xl:` grid and the JS `DEVICES_SPLIT_QUERY` are the same
number and must flip together** — pairing's default-open state is tied to the layout, so a mismatch
of one tier produces a 360px column containing nothing but a collapsed title.

Pairing stays *in the page* in both tiers — not a drawer, not a sub-route. During
pairing the user moves between two surfaces (send my invite out, paste theirs back in), and an
overlay repeatedly covers the paired-device list, which is exactly where they are watching for the
result. A sub-route is the same problem taken further: the whole list is gone. Splitting improves on
the older in-place disclosure — both blocks are visible *at once*, so a newly paired device appears
in the next column without collapsing anything first. Desktop diverges here (its pairing lives at
`/pairing/generate` and `/pairing/input`) because its right column carries nearby-device discovery
as well, and the browser has no mDNS to put there.

**The one exception is the identity confirmation, which *is* a dialog** (`PairingConfirmDialog`).
The paragraph above is about the pairing *panel* — the surface the user moves around in. Confirming
"this invite decodes to device X, do I trust it" is not a surface, it is a single decision taken
once, and the reason against overlays does not hold for it: what the user needs to read at that
instant is the peer's identity, not the device list behind it, and the dialog closes the moment they
answer — the newly paired device is then already in the list underneath. Web has the same decision in
both directions (`PairingRequestHost` for inbound requests, this for outbound), and they must look
alike; keeping the outbound half inline made it a 360px card with a truncated NodeId while the
inbound half got a full dialog. **A 360px column cannot hold the information density an identity
check needs** — the full 52-character NodeId alone does not fit. Everything else about pairing —
invite generation, the QR, the outstanding-invite list — stays in the column.

**Section names are cross-platform.** The same concept gets the same title and icon in every build:
web's settings say 设备信息 / 引导节点 with `MonitorSmartphone` / `RadioTower` because desktop does.
Renaming is a **presentation-layer** change only — the web build still calls its component
`ConnectionPanel` and its store domain `relays`, because those are kernel facts (it really does
register relays). Two names for one thing in the UI makes users think there are two things to
configure; two names between UI and kernel is just accurate.

#### The icon table

"…and icon" went unenforced until 2026-08-07, when a sweep found the same concept drawn three
different ways across the builds. These are the bindings; changing one means changing three.

| Concept | Icon | Note |
|---|---|---|
| Transfers (nav, empty state, section) | `ArrowLeftRight` | Not `ArrowRightLeft` — desktop had the mirrored twin, mobile's transfer-history empty state had `Activity` |
| Inbox | `Inbox` | Mobile drew all three of its inbox empty states as `FileArchive`, i.e. a zip file |
| Devices (the *set* — nav, empty state, picker) | `MonitorSmartphone` | A single device still uses its platform icon |
| Send action | `Send` | Not `SendHorizontal` |
| Direction: sending / receiving | `ArrowUpFromLine` / `ArrowDownToLine` | Three builds had four pairs, including two inside mobile alone |
| Pairing / invite | `Link2` · `QrCode` · `ClipboardPaste` | |
| Groups | `Tags` | |
| Bootstrap / relay | `RadioTower` | |
| Device platform | `Monitor` win+linux · `Laptop` mac · `Smartphone` ios+android | Mobile drew Windows as `Laptop`; its extra `Tablet` branch is fine — it sits before a fallback the others share |
| File types | `packages/file-browser/src/file-icon.ts` | Unknown type is `File`, **not** `FileArchive` |
| Settings sections | `Network` · `HardDrive` · `MonitorSmartphone` · `Bot` · `RadioTower` · `Palette` · `Info` | |
| Docs · repo · changelog | `BookText` · `Github` · `ScrollText` | `Github` is a lucide brand icon that **`lucide-react-native` no longer ships**, so mobile uses `Code` there — a library limit, recorded so nobody "fixes" it back |

State colors bind the same way: the four trust levels are `primary` (owned) / `muted`
(collaborator) / `warning` (temporary) / `destructive` (blocked) in all three builds. Collaborator
is the *default* level — coloring it, as mobile did with `success`, tints most of a device list.

### Cross-platform UI Review Checklist

Run this when adding or changing device-related UI in **any** build. It is a gate before visual
review, not after.

- [ ] All eight Device Card slots present (or explicitly absent because the data is)
- [ ] Online state shows a dot **and** a word
- [ ] Node status: conclusion layer carries a consequence sentence, at most one CTA, and the status
      word comes from `summarizeNodeHealth` — not a locally invented "good / limited" synthesis
- [ ] Node status: `lastError` shown verbatim, copyable, untranslated; per-link failures do not
      color the persistent slot
- [ ] Latency rendered whenever the device is online and `connection` is known
- [ ] Active transfer surfaces carry all four progress slots — or drop one against the slot table's
      rules (bytes first, speed second; **ETA is never the one that goes**), and ETA renders a
      placeholder rather than vanishing when it cannot be computed
- [ ] Any local phase that can exceed ~3 s (prepare, publish) names itself, and shows a percentage
      wherever the phase has a measurable loop — a constant-time rename/close only names itself
- [ ] Send reachable from the device card, target pre-selected
- [ ] Offline devices: send disabled, whole-card click disabled, visual degradation applied
- [ ] Destructive actions (unpair) require one explicit confirmation, and can report failure
- [ ] Display name / grouping / trust normalization come from `@swarmdrop/shared-view`, not a local copy
- [ ] Every interactive control reachable without hover; touch targets ≥ 44×44 CSS px
- [ ] New user-facing strings go through that build's i18n, including `aria-label` / `title` / `alt`
- [ ] Icons match the icon table above — a concept the other builds already draw keeps their glyph
- [ ] No raw palette class (`bg-green-500`, `text-amber-600`, `bg-zinc-950`) — state goes through
      `success` / `warning` / `destructive` / `info` + the `-ink` text form. The three standing
      exceptions are the QR white card, the camera scanner, and the theme-preview thumbnails:
      all three are deliberately theme-vacuum surfaces and say so in a comment.

### Navigation — Desktop shell (Topbar + Breadcrumb)
- Desktop uses a single top bar: an unclickable logo mark, a node-status pill, a breadcrumb trail (home icon → intermediate clickable segments → unclickable current page), and window controls. There is no persistent sidebar in the current build — navigation depth is expressed through the breadcrumb, not through a nav rail.
- The topbar's only structural line is a 1px `rgb(255 255 255 / 0.34)` (light) / `rgb(255 255 255 / 0.08)` (dark) bottom hairline — no shadow, no background fill of its own beyond the ambient shell.

### Navigation — Web app area (Sidebar + Bottom Nav)
The browser build (`docs/app/app`, hosted inside the docs site) uses a **persistent sidebar**, not the breadcrumb-only topbar. This is a deliberate fork, decided in issue #88 — it is the "check against the current breadcrumb-only pattern" the Don't-list below asks for, and its conclusion applies to the Web area only: **the desktop shell does not change.**

Why the fork: the desktop app owns its entire window and can borrow the native title bar for chrome, so a breadcrumb is enough to say "you are inside an app". A browser tab has no such frame — the same tab renders marketing pages and docs — so persistent nav is the only structure that reads as an application rather than another document page. Deep-linkable routes also require a visible place to return to.

- **Sections** mirror the desktop information architecture — devices / send / inbox / transfer / settings — so the two builds stay conceptually one product. Only the navigation *shape* differs.
- **Persistent nav lists three of them: devices / inbox / settings.** Send and transfer are *sub-pages of devices*: entered from the devices page, they keep "devices" highlighted in the rail and state the parent in the page title as a breadcrumb (`设备 › 传输`, the parent segment being the link back). That title form is shared with the desktop shell on purpose: what the web build forks is the shape of the *persistent* navigation (rail vs breadcrumb trail), not how a page states its own parent — that should read the same in both. It replaced a separate `← 设备` row above the title, which put the visual weight of a two-line header on a small back arrow. This matches what the other two builds already did — the desktop topbar has no send entry, and the mobile tab bar has neither send nor transfer. Send in particular must never become a persistent nav item: the Send Entry Contract above says sending starts from a device, so a standing entry can only land the user on the target picker that exists for *correcting* a target. The parent/child relation lives in `docs/app/app/_lib/nav.ts`; giving an item a `parent` removes it from the rail.
- **Three responsive tiers:** ≥1024px expanded rail (icon + label, 224px) · 768–1023px icon rail (64px, label degrades to `title`/`aria-label`) · <768px bottom nav with a brand+status header above the content. The single source of truth for the items is `docs/app/app/_lib/nav.ts`. Neither the header nor the bottom nav is `fixed`/`sticky` any more: the app shell is an `h-dvh` flex container and scrolling happens inside `main`, so both are ordinary `shrink-0` children that stay put on their own — which also removed the height constant that a `fixed` bar had to be compensated for.
- **The brand mark is the way back to the marketing site** (`/`), and it is the web build's one departure from desktop's "unclickable logo mark" — trivially so, since the desktop shell has no marketing site to return to. The app area is a room inside the docs site with no door out: all three rail items live under `/app`, and the only other in-site link is "使用文档" in the bottom menu, which goes to docs rather than home. Clicking the logo to get home is the one browser convention nobody has to be taught, so the missing door hangs there. The link carries the tier padding (`min-h-11`, and the header row gives up its horizontal padding to it) so its hit area matches a nav item's — 207×44 expanded, 47×44 in the icon rail — and the mark lands on the same 20px left edge as the nav icons below it. Its accessible name states the destination ("SwarmDrop 官网首页"); in the icon rail the label is `display:none`, so that name is the only one there is.
  - **Leaving `/app` does not interrupt transfers**, contrary to what the code said for a long time. The node runtime is a *module-level* singleton whose lifetime follows the page, not the React tree: `WebNodeBootstrap` has no cleanup by design, and the only call site of `closeNode()` is the "stop node" button in the status dialog. A client-side link out of the app area unmounts DOM and nothing else. Verified 2026-08-06 by round-tripping `/app/devices → /docs →` back in one document and sampling the status pill every 100ms for 4s — it never left "运行中", which is a tight test because stopping the node also resets the store. The retracted claim ("leaving `/app` unmounts the node singleton and interrupts transfers") had appeared in two component headers and cost a design decision before it was checked.
- **Active state** is `bg-fd-accent` + `text-[var(--brand)]` + `aria-current="page"` — the one-accent rule still holds; no second saturated color enters the chrome.
- **The count badge** (pending offers) uses the brand solid fill with `--brand-ink` text. It exists because splitting one page into five routes hides time-sensitive inbound requests behind a route — the badge is the compensation for that, not decoration. In-flight transfers get the same compensation in a different form: the devices page carries an "active transfers" section with live rows, which is what replaced their nav badge when transfer left the rail. **Whatever is taken out of the chrome has to reappear somewhere the user already is** — that rule is what both of these are instances of.
- **Node status stays visible in every tier** (pill when there's room, bare status dot in the icon rail): "state is honestly visible" does not get dropped because the window got narrow. The pill is also the **entry point to node control** — status, uptime, relay reachability and diagnostics, plus start/stop. Node control is deliberately *behind* it rather than on any page: visible when looked for, never stumbled into.
- **The rail costs 224px of content width, and page layouts must budget for it.** This is the one structural consequence of the fork that is easy to forget: a viewport width that splits comfortably on desktop may not on web. The devices page's 1280 split breakpoint is the worked example — see "The devices page splits at 1280px" under the Layout Density Contract.

### Page overview stats (devices)

The devices page header carries three counters on its right — **online / paired / in-transit**.
Desktop's equivalent (`HomeOverview`) reads *nearby* / paired / in-transit; **web replaces "nearby"
because it does not exist there** (mDNS discovery is a native capability). A counter that is
permanently zero is worse than an absent one: it reads as "nothing is nearby" rather than "this
build does not look".

"Online" earns its slot on its own: offline devices offer no send action (Send Entry Contract), so
"how many can I actually reach right now" is the page's most load-bearing number, and the section
header's count only gives the total. Paired overlaps that count deliberately — with only "online 2"
a user cannot tell 2-of-2 from 2-of-9.

**It goes in the existing page header, not in a banner of its own.** Desktop's overview block
carries its own title and positioning line; the web `PageHeader` already renders both, so a separate
block would say the same sentence twice and cost a screenful of height. Desktop's block is itself
"title left, stats right" — web is adding the missing half, not cloning the whole.

### Ambient WebGL Background (signature component)
A `Renderer`-driven (`ogl`) full-bleed canvas sits behind every app screen: a slow Perlin-noise "soft aurora" gradient (`aurora-mist` → `aurora-cyan`) always on, plus a teal/light-blue "side rays" overlay that appears only in dark mode. The loop is gated by `IntersectionObserver` + `visibilitychange` (pauses when off-screen or the tab is hidden) and fully respects `prefers-reduced-motion` by freezing on the first frame instead of skipping the effect outright — the texture stays, the motion doesn't. This is the system's single biggest personality investment; everything else in the UI stays deliberately quiet so this can carry the "alive network" feeling.

**Desktop and web both ship it** (web since 2026-08-05 — see the overturned bullet in
"Cross-platform token unification"). Two implementations, one standard: the **shaders and the
`*_CONFIG` blocks are copied verbatim** between `src/components/layout/app-ambient-background.tsx`
and `docs/app/app/_components/ambient-canvas.tsx`. Never tune one side only — a drifted aurora is
invisible in isolation (both are "a moving light") and only shows up side by side.

The web deltas are all forced by the browser baseline, not by taste:

| | Desktop | Web |
|---|---|---|
| Load | direct import | `next/dynamic` + `ssr:false` — pulling `_bg.wasm` to start the node outranks decoration. Measured: 15.6 KB gzip, absent from the route's first-load chunks |
| DPR | aurora unset (1), rays capped at 2 | both capped by `ambientDpr()`: 1 below 768px, else ≤1.5 |
| Frame rate | full RAF | throttled to 30 fps. The motion is second-scale; 30 and 60 are indistinguishable and the GPU work halves |
| Layer opacity | — | **Shared, no longer a delta:** dark 1 · light 0.34 |
| Mask | — | **Shared:** radial `mask-image`, two strengths by theme. Dark keeps 45% at the centre so panels have something to refract; light hollows it out entirely |

**Opacity and mask are now shared by both builds** (2026-08-06). They were a web-only delta, which
had it backwards in both directions: web's dark value (0.7) held the aurora below the level at
which glass works at all, while desktop ran unmasked and let the shader's mid-height band cross the
card area as the brightest thing on screen. An ambient layer's job is to be *sensed*, not seen
first — "everything else stays quiet so this can carry the feeling" means legible, not loud.

**Attenuate by position, not by amount.** Turning opacity down removes the light *behind the
panels* too, and that light is the only thing glass has to work with. The mask presses on the
centre, which is what the band problem actually called for.

**The light/dark split that remains is a measured constraint, not a preference** (the table also
read "0.7 dark / 0.48 light" for a while when the code said 0.34 — three places stating this, so
check all three when changing it).

Additive light on a near-white surface has almost no headroom before `--muted-foreground` on a
glass card drops under WCAG AA. Sampled by colour mode over the text's own background, worst of
3 frames — the layer drifts, so **single-frame sampling lies** (one frame of the 0.34 + soft-mask
combination measured 4.566 and looked safe; its worst frame was 4.413):

| Light configuration | Worst | |
|---|---|---|
| 0.34 + hollow mask | **4.645** | 3.2% headroom — shipped |
| 0.24 + soft mask | 4.518 | 0.4% headroom, i.e. none |
| 0.28 / 0.34 + soft mask | 4.443 / 4.413 | below AA |

Dark has room to spare: full strength with the soft mask measures **7.3–8.4:1**. Desktop light,
which previously ran unmasked at full strength, measured 4.693 before and 4.653 after — the mask
moves it toward safety, not away.

**Reduced-transparency drops the ambient layer entirely** (`display: none`), whereas
reduced-motion freezes the first frame. The two preferences ask different questions: motion asks
for stillness (keep the texture), transparency asks not to look through things — and once glass
degrades to a flat fill, nothing behind it is visible anyway.

**Mount it at the shell only.** It holds a WebGL context plus a RAF loop; per-route instances hit
the browser's live-context cap, which fails by silently discarding the oldest context.

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
