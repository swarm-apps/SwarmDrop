---
name: SwarmDrop Mobile
description: Cross-network, end-to-end encrypted file transfer between your own devices.
colors:
  trust-blue: "#3B82F6"
  porch-white: "#FFFFFF"
  doorway-ink: "#020817"
  quiet-surface: "#F1F5F9"
  quiet-ink: "#0F172A"
  hush-gray: "#64748B"
  threshold-line: "#E2E8F0"
  welcome-green: "#16A34A"
  caution-amber: "#F59E0B"
  alert-red: "#EF4444"
typography:
  display:
    fontFamily: "System (SF Pro on iOS / Roboto on Android)"
    fontSize: "30px"
    fontWeight: 700
    lineHeight: "36px"
    letterSpacing: "normal"
  title:
    fontFamily: "System"
    fontSize: "15px"
    fontWeight: 600
    lineHeight: "20px"
    letterSpacing: "normal"
  body:
    fontFamily: "System"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: "20px"
    letterSpacing: "normal"
  label:
    fontFamily: "System"
    fontSize: "11px"
    fontWeight: 500
    lineHeight: "14px"
    letterSpacing: "normal"
rounded:
  sm: "6px"
  md: "8px"
  lg: "10px"
  xl: "12px"
  full: "9999px"
spacing:
  xs: "2px"
  sm: "6px"
  md: "12px"
  lg: "14px"
components:
  button-primary:
    backgroundColor: "{colors.trust-blue}"
    textColor: "{colors.quiet-ink}"
    typography: "{typography.body}"
    rounded: "{rounded.xl}"
    padding: "10px 16px"
    height: "40px"
  button-outline:
    backgroundColor: "{colors.porch-white}"
    textColor: "{colors.doorway-ink}"
    typography: "{typography.body}"
    rounded: "{rounded.xl}"
    padding: "10px 16px"
    height: "40px"
  card:
    backgroundColor: "{colors.porch-white}"
    textColor: "{colors.doorway-ink}"
    rounded: "{rounded.lg}"
    padding: "14px"
  device-row:
    backgroundColor: "{colors.porch-white}"
    textColor: "{colors.doorway-ink}"
    typography: "{typography.title}"
    rounded: "{rounded.lg}"
    padding: "14px"
    height: "76px"
  status-pill-running:
    backgroundColor: "{colors.welcome-green}"
    textColor: "{colors.welcome-green}"
    typography: "{typography.label}"
    rounded: "{rounded.full}"
    padding: "4px 10px"
  trust-badge-owned:
    backgroundColor: "{colors.trust-blue}"
    textColor: "{colors.trust-blue}"
    typography: "{typography.label}"
    rounded: "{rounded.full}"
    padding: "2px 10px"
---

# Design System: SwarmDrop Mobile

## 1. Overview

**Creative North Star: "The Trusted Doorstep"**

SwarmDrop Mobile is the doorstep of a home you control: you decide who gets a key, you can see who's standing there, and nothing gets in — or out — without your say-so. The system is built around **control through visibility, not through complexity**. Every screen answers one of three questions at a glance: who is this device to me, is it here right now, and what's happening with my files. It does this in a friendly, low-key voice, never a hardened-security one — the app should feel like a considerate host, not a bouncer.

This system explicitly rejects the two poles PRODUCT.md names as anti-references: it is not a social/entertainment feed (no hero avatars, no likes, no timelines), and it is not an enterprise SaaS console (no dense data tables, no dashboard stacking). Both would bury the doorstep metaphor under either performative flourish or administrative weight.

**Key Characteristics:**
- Neutral, near-monochrome surfaces; color is spent almost entirely on *state* (online, trusted, transferring, failed), never on decoration.
- A tight 10–15px type scale carries nearly all product UI; size barely moves, weight and color do the talking.
- Flat at rest, lifted only when a surface floats above the page (sheets, dialogs, menus).
- Every interactive surface answers instantly (`active:opacity-70`), no hover-dependent affordances — this is a touch-first surface.

## 2. Colors

The palette is a neutral shadcn/ui "New York" base (unmodified — this project has not yet diverged from the stock preset) carrying four semantic accents. Read this as two layers: a quiet neutral scaffold, and a small, disciplined set of state colors layered on top.

### Primary
- **Trust Blue** (`#3B82F6`, `hsl(217.2 91.2% 59.8%)`): the one color that means "act here" — primary buttons, the send affordance on a device row, focus rings, and the "owned device" trust badge. It is the same hex in light and dark mode; only its foreground text flips (see Named Rule below).

### Neutral
- **Porch White** (`#FFFFFF`): light-mode background, card, and popover surface.
- **Doorway Ink** (`#020817`): light-mode body text; becomes the dark-mode background.
- **Quiet Surface** (`#F1F5F9`): secondary/muted/accent backgrounds — icon chips, pill backgrounds at rest, disabled fills.
- **Quiet Ink** (`#0F172A`): text on Quiet Surface; also doubles as `--primary-foreground` in both light and dark mode (see Named Rule).
- **Hush Gray** (`#64748B`): muted/secondary text — captions, offline-state labels, "os · platform" metadata rows.
- **Threshold Line** (`#E2E8F0`): every border and input outline in the app. One border color, no exceptions — it is the literal edge of the doorstep.

### State (used only for status, never decoratively)
- **Welcome Green** (`#16A34A` light / `#22C55E` dark): device online, transfer complete, "collaborator" trust level.
- **Caution Amber** (`#F59E0B` light / `#FACC15` dark): node starting up, transfer in progress.
- **Alert Red** (`#EF4444` light / `#7F1D1D` dark): errors, blocked devices, destructive actions.

Dark mode swaps background/foreground (Porch White ↔ Doorway Ink) and deepens neutrals one step (Quiet Surface → `#1E293B`, Threshold Line → `#1E293B`, Hush Gray → `#94A3B8`), while Trust Blue, Welcome Green, and Alert Red each get a dedicated, slightly adjusted dark-mode value already defined in `src/global.css` — never a flat opacity hack over the light value.

### Named Rules
**The Doorstep Threshold Rule.** There is exactly one border/input color in the entire system (`--border` / `--input`, both `#E2E8F0` light, `#1E293B` dark). Never introduce a second gray for dividers — reach for opacity on existing semantic colors (`bg-success/10`, `bg-destructive/15`) instead of a new hex.

**The Unified Ink Rule.** `--primary-foreground` is dark ink (`#0F172A`) in both light and dark mode, even though `--primary` itself (`#3B82F6`) never changes. This used to flip (light text in light mode, dark text in dark mode) until an audit found the light-mode pairing cleared only ~3.5:1 contrast against Trust Blue — below the 4.5:1 AA floor. Dark ink clears ~4.9:1 in both modes, so the token is now one value, not two. Don't reintroduce a lighter light-mode variant without re-verifying contrast first.

**The State Ink Rule.** The saturated state colors (`--success` / `--warning` / `--destructive` / `--primary`) are calibrated for **fills, dots, and icons**, not for text. As *small text* on a light surface or a same-hue 10–15% tint they clear only ~2:1 (amber), ~3.3:1 (green), ~3.5:1 (red), ~3.4:1 (blue) — all below AA. So there is a dedicated **ink** token per state for text use: `--success-ink` / `--warning-ink` / `--destructive-ink` / `--primary-ink` (light = a darker shade, e.g. amber-700 / green-700 / red-700 / blue-600; dark = a lighter shade for text on dark tint). All clear ≥4.5:1 on both white and the `/10`–`/15` tint. **Rule:** state color as a *dot / fill / icon* → the base token; state color as *text* → the `-ink` token. `StatusPill`, `TrustBadge`, `ConnectionBadge`, and `transfer/shared`'s `StatusBadge` all follow this; new state text must too.

## 3. Typography

**Font:** System default — SF Pro on iOS, Roboto on Android. No custom font is loaded anywhere in the app; this is deliberate neutrality, not an oversight.

**Character:** Restrained and dense. This is a control surface for devices and trust relationships, not an editorial or marketing surface — so the scale stays small and the weight/color axis carries almost all the emphasis.

### Hierarchy
- **Display** (bold 700, 30px, `tabular-nums`): reserved for exactly one place today — the live transfer progress number on the transfer detail screen. Not a heading style; a metric readout.
- **Headline** (semibold 600, 24–30px, `tracking-tight`): available via the `h2`/`h3` Text variants but **not currently used on any product screen** — treat as available inventory for future section headers, not an established pattern.
- **Title** (semibold 600, 15px): the real workhorse heading — device names, primary list-row titles, section titles. This is the size a user's eye lands on first.
- **Body** (regular 400, 14px, occasionally 16px `text-base` for prose-like copy): button labels, secondary row text, settings descriptions.
- **Label** (medium 500, 10–11px): status pills, trust badges, connection badges, "os · platform" captions. This is the densest, most frequently repeated text style in the app.

### Named Rules
**The Small-Print Confidence Rule.** 80%+ of all text sizes in this codebase fall between 10px and 15px (`text-[11px]` alone appears 80+ times). Do not reach for a larger size to add emphasis — reach for `font-semibold` or a state color instead. A 30px+ size is earned only by a genuine metric readout, never by a section header wanting attention.

## 4. Elevation

Two-tier, deliberately flat-first. Resting surfaces (buttons, cards, inputs, device rows) carry only a whisper of shadow (`shadow-sm shadow-black/5`) — close enough to flat that depth reads mainly through the Threshold Line border and the Quiet Surface fill, not through cast shadow. Elevation is reserved for content that floats *above* the page: bottom sheets, dialogs, popovers, dropdown menus, and the select control step up to a visibly stronger shadow so they read as temporarily suspended above the doorstep, not as another tile on it.

### Shadow Vocabulary
- **Resting** (`shadow-sm`, `shadow-black/5`): buttons, cards, badges, inputs, device rows. Default for anything anchored in the layout flow.
- **Floating** (`shadow-lg` / `shadow-md`): dialog, alert-dialog, popover, dropdown-menu, select. Anything that overlays the page on open.

### Named Rules
**The Almost-Flat Rule.** If a resting component's shadow is visible without close inspection, it's too strong — `shadow-black/5` is the ceiling for anchored content. Only floating/overlay surfaces earn a shadow you can actually see.

## 5. Components

### Buttons
- **Shape:** `rounded-xl` (12px) — the app's canonical action-button radius. The shadcn `<Button>` primitive and all hand-rolled `Pressable` buttons now agree on it; cards/surfaces stay `rounded-lg` (10px). (This was reconciled from an earlier `rounded-md`/`lg`/`xl`/`2xl` spread that made the same action look different screen-to-screen.)
- **Primary:** Trust Blue fill, **dark-ink text** (`--primary-foreground`, `#0F172A`, per the Unified Ink Rule — not white), `h-10` (40px, `sm:h-9`), `px-4 py-2`. Active feedback is either the variant fill (`active:bg-primary/90`, shadcn `<Button>`) or `active:opacity-70` (hand-rolled Pressable) — never a hover-only affordance, this is touch-first.
- **Interaction consistency:** hand-rolled action buttons standardize on `active:opacity-70` + `disabled:opacity-50` (never `active:opacity-80/90` or `disabled:opacity-55`).
- **Outline:** Porch White fill with a Threshold Line border; active state fills to Quiet Surface (`active:bg-accent`).
- **Ghost:** no fill or border at rest; active state fills to Quiet Surface at half strength.
- **Destructive:** Alert Red fill, white text; used sparingly (block device, delete history).
- **Sizes:** `sm` (36px), `default` (40px), `lg` (44px) — all `sm:` variants drop 4px for wider layouts.

### Badges & Pills
- **Trust Badge:** 4 levels (owned / collaborator / temporary / blocked), each a 10%-tint pill (`bg-{color}/10` or `/15`) in `rounded-full`, `text-[10-11px] font-medium`. A `confirmed: false` state appends "· 待确认" inline rather than changing color — pending state is a text suffix, not a new hue.
- **Status Pill:** 4 states (running/starting/stopped/error) — colored dot + label, same tint pattern as Trust Badge. Doubles as a tap target when `onPress` is supplied (starts/stops the node) — the same visual object serves as both indicator and control.
- **Connection Badge:** compact variant only seen alongside Trust Badge on online devices — latency/connection-type readout in Label scale.

### Cards / Device Rows
- **Corner style:** `rounded-lg` (10px).
- **Background:** Porch White, `border` in Threshold Line.
- **Padding:** `p-3.5` (14px) — one consistent inset across card and row variants.
- **Two variants:** `row` (76px min-height, horizontal, for the primary phone list) and `card` (vertical, for wider grid layouts) — same visual language, different flow direction.
- **Offline state:** `opacity-60`–`opacity-65` on the whole row/card rather than a separate "disabled" color — the doorstep still shows you the device, just dimmed, not hidden.
- **Send affordance:** a `size-11` (44px) circular Trust Blue button embedded at the trailing edge, disabled (falls back to Quiet Surface fill) when the device can't currently receive.

### Inputs
- **Style:** Threshold Line border, Porch White background (`dark:bg-input/30`), `rounded-md`, `h-10` (`sm:h-9`).
- **Focus (web only):** ring in Trust Blue at 50% opacity, 3px.
- **Disabled:** `opacity-50`.

### Navigation
- Bottom tabs use `expo-router`'s native tab bar (system-rendered, not a JS-drawn bar) — system handles safe-area, ripple, and transition; theme color is injected via `backgroundColor`/`iconColor`/`labelStyle` rather than custom-drawn chrome.

## 6. Do's and Don'ts

### Do:
- **Do** keep color spend almost entirely on state (online/offline, trust level, transfer status) — color is information, not decoration.
- **Do** use the 10–15px type scale for anything that repeats on screen; reserve 24px+ for the rare, genuinely singular metric readout.
- **Do** use `active:opacity-70` (or the variant-specific active fill) as the only interaction feedback — no hover-dependent states on native.
- **Do** keep resting surfaces at `shadow-sm shadow-black/5` or flatter; reserve visible shadow for anything that floats above the page on open (dialog, sheet, popover, dropdown, select).
- **Do** dim offline/disabled states with opacity on the whole surface, not a separate muted color scheme.

### Don't:
- **Don't** introduce social/entertainment patterns — hero avatars, like counts, or timeline-style activity feeds. Per PRODUCT.md, this is explicitly rejected.
- **Don't** build enterprise-SaaS-style dense data tables or dashboard-stacking layouts. Per PRODUCT.md, this is explicitly rejected.
- **Don't** add a second border/divider gray — every edge in this app is Threshold Line (`#E2E8F0` / `#1E293B` dark); reach for opacity variants of existing state colors instead.
- **Don't** revert `--primary-foreground` back to a lighter light-mode value — the unified dark-ink value (see the Unified Ink Rule) is what clears WCAG AA; a lighter value only cleared ~3.5:1.
- **Don't** add decorative gradients, glassmorphism, or gradient text anywhere — none fit a doorstep that's supposed to feel calm and legible, not flashy. (The single exception is `ios-toast`'s `BlurView`: a floating, transient, iOS-native toast surface — glass on a momentary overlay, never on resting content.)
