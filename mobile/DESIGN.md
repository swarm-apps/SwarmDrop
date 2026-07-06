---
name: SwarmDrop Mobile
description: Cross-network, end-to-end encrypted file transfer between your own devices.
colors:
  harbor-teal: "#0F8F7A"
  action-teal: "#087968"
  harbor-teal-ink: "#087968"
  copper-core: "#C56A42"
  brand-mist: "#F0FBF7"
  mist-surface: "#F5FAF8"
  porch-white: "#FFFFFF"
  doorway-ink: "#020817"
  night-background: "#121E20"
  night-surface: "#18282B"
  night-muted: "#203538"
  quiet-surface: "#F1F5F9"
  quiet-ink: "#0F172A"
  hush-gray: "#64748B"
  threshold-line: "#DDEAE6"
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
    backgroundColor: "{colors.action-teal}"
    textColor: "{colors.porch-white}"
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
    backgroundColor: "{colors.mist-surface}"
    textColor: "{colors.doorway-ink}"
    rounded: "{rounded.lg}"
    padding: "14px"
  device-row:
    backgroundColor: "{colors.mist-surface}"
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
    backgroundColor: "{colors.harbor-teal}"
    textColor: "{colors.harbor-teal-ink}"
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

The palette is a quiet neutral scaffold with white page chrome, mist-tinted content surfaces, and a small, disciplined set of state colors layered on top.

### Primary
- **Harbor Teal** (`#0F8F7A`, `hsl(170.2 81% 31%)`): the selected logo subject color. It is the brand anchor, but it is too light for small white labels on buttons.
- **Action Teal** (`#087968`, `hsl(171 87.6% 25.3%)`): the mobile UI action fill for primary buttons, send affordances, checked controls, and light-mode focus rings. It is one step deeper than the logo teal so button labels and lucide icons can be white and still clear ~5.3:1 contrast.
- **Copper Core** (`#C56A42`): the small secondary brand color in the logo center. It adds warmth to the mark, but it is not a general UI action or state color.

### Neutral
- **Porch White** (`#FFFFFF`): light-mode app background and popover surface. The page stays clean and familiar instead of looking like a tinted marketing screen.
- **Mist Surface** (`#F5FAF8`): light-mode cards, device rows, and repeated content surfaces. It is deliberately softer than the brand mist so repeated lists do not turn the whole app green.
- **Brand Mist** (`#F0FBF7`): app icon / splash background and occasional brand panels only, not the default repeated card color.
- **Doorway Ink** (`#020817`): light-mode body text.
- **Night Background** (`#121E20`): dark-mode app background; softened away from near-black so large mobile surfaces do not feel harsh.
- **Night Surface** (`#18282B`): dark-mode card and popover surface.
- **Night Muted** (`#203538`): dark-mode secondary/muted/accent fills, borders, and inputs.
- **Quiet Surface** (`#F1F5F9`): secondary/muted/accent backgrounds — icon chips, pill backgrounds at rest, disabled fills.
- **Quiet Ink** (`#0F172A`): text on Quiet Surface.
- **Hush Gray** (`#64748B`): muted/secondary text — captions, offline-state labels, "os · platform" metadata rows.
- **Threshold Line** (`#DDEAE6` light / `#203538` dark): every border and input outline in the app. One border color, no exceptions — it is the literal edge of the doorstep.

### State (used only for status, never decoratively)
- **Welcome Green** (`#16A34A` light / `#22C55E` dark): device online, transfer complete, "collaborator" trust level.
- **Caution Amber** (`#F59E0B` light / `#FACC15` dark): node starting up, transfer in progress.
- **Alert Red** (`#EF4444` light / `#7F1D1D` dark): errors, blocked devices, destructive actions.

Dark mode uses a softened teal-black stack instead of pure black: Night Background `#121E20`, Night Surface `#18282B`, Night Muted `#203538`, and Hush Gray `#9AB0AE`. Action Teal remains the primary fill so button text/icons stay white across themes; the dark focus ring uses the brighter brand cyan (`#5EE0C8`) for visibility.

### Named Rules
**The Doorstep Threshold Rule.** There is exactly one border/input color in the entire system (`--border` / `--input`, both `#DDEAE6` light, `#203538` dark). Never introduce a second divider color — reach for opacity on existing semantic colors (`bg-success/10`, `bg-destructive/15`) instead of a new hex.

**The Primary Button Contrast Rule.** Mobile primary buttons use Action Teal (`#087968`) as the fill and white as `--primary-foreground`. Do not put white text/icons on raw Harbor Teal (`#0F8F7A`) for small buttons: it clears only ~4.0:1. If a button needs white labels or lucide icons, use Action Teal as the fill.

**The State Ink Rule.** The saturated state colors (`--success` / `--warning` / `--destructive` / `--primary`) are calibrated for **fills, dots, and icons**, not for text. As *small text* on a light surface or a same-hue 10–15% tint they can fall below AA — raw Harbor Teal is only ~4.0:1 as small text on white, while `--primary-ink` (`#087968`) clears ~5.3:1. So there is a dedicated **ink** token per state for text use: `--success-ink` / `--warning-ink` / `--destructive-ink` / `--primary-ink` (light = a darker shade; dark = a lighter shade for text on dark tint). All clear ≥4.5:1 on both white and the `/10`–`/15` tint. **Rule:** state color as a *dot / fill / icon* → the base token; state color as *text* → the `-ink` token. `StatusPill`, `TrustBadge`, `ConnectionBadge`, and `transfer/shared`'s `StatusBadge` all follow this; new state text must too.

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
- **Primary:** Action Teal fill, **white text/icons** (`--primary-foreground`, `#FFFFFF`, per the Primary Button Contrast Rule), `h-10` (40px, `sm:h-9`), `px-4 py-2`. Active feedback is either the variant fill (`active:bg-primary/90`, shadcn `<Button>`) or `active:opacity-70` (hand-rolled Pressable) — never a hover-only affordance, this is touch-first.
- **Interaction consistency:** hand-rolled action buttons standardize on `active:opacity-70` + `disabled:opacity-50` (never `active:opacity-80/90` or `disabled:opacity-55`).
- **Paired-action layout (dialogs & confirm screens):** two-action sets are ALWAYS a horizontal row — dismissive (outline) left, confirming (primary/destructive) right, equal `flex-1` widths (iOS HIG / Material 3 / 微信系惯例). `AlertDialogFooter`/`DialogFooter` were de-webbed from shadcn's `flex-col-reverse sm:flex-row` for this; vertical stacking is reserved for ≥3 actions or menu-style row lists (e.g. the inbox long-press sheet). Destructive-adjacent secondary buttons keep the neutral `border-border` — danger is carried by red ink (icon + text), never by a tinted border.
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
- **Background:** Mist Surface, `border` in Threshold Line.
- **Padding:** `p-3.5` (14px) — one consistent inset across card and row variants.
- **Two variants:** `row` (76px min-height, horizontal, for the primary phone list) and `card` (vertical, for wider grid layouts) — same visual language, different flow direction.
- **Offline state:** `opacity-60`–`opacity-65` on the whole row/card rather than a separate "disabled" color — the doorstep still shows you the device, just dimmed, not hidden.
- **Send affordance:** a `size-11` (44px) circular Action Teal button embedded at the trailing edge, disabled (falls back to Quiet Surface fill) when the device can't currently receive.

### Inputs
- **Style:** Threshold Line border, Porch White background (`dark:bg-input/30`), `rounded-md`, `h-10` (`sm:h-9`).
- **Focus (web only):** ring in Action Teal at 50% opacity, 3px; dark mode uses the brighter brand cyan ring token.
- **Disabled:** `opacity-50`.

### Navigation
- Bottom tabs use `expo-router`'s native tab bar (system-rendered, not a JS-drawn bar) — system handles safe-area, ripple, and transition; theme color is injected via `backgroundColor`/`iconColor`/`labelStyle` rather than custom-drawn chrome.

### Named Rules
**The Radius Vocabulary Rule.** 全 app 的圆角语义,别再发明新的:
- **动作按钮**(含图标方钮如 `HeaderIconButton`、行内小方钮)→ `rounded-xl`(12px);
- **Surface / 卡片 / 成组列表容器 / 信息 pill / 输入槽**(含 OTP 槽)→ `rounded-lg`(10px)为标准;
- **身份类图标 chip**(设备平台图标、弹窗头图标、空态大图标——"这是谁/这是什么东西")→ `rounded-full`;
- **内容类型 / 行首图标 chip**(收件箱行、文件行、操作 sheet 行的行首小方块——"这行是什么类型")→ `rounded-xl`(既有一致模式,保持);
- **徽标 / 状态 pill / 状态点** → `rounded-full`。

例外与已知漂移:① 关于页的 "SD" 标志块保留 `rounded-2xl`——模仿系统 App 图标的 squircle,是身份图形不是 chip;② shadcn `ui/card` 原语、传输详情/设备详情的部分成组容器、弹窗内衬 muted 块仍是 `rounded-xl`——与 Surface 的 `rounded-lg` 存在 10/12px 分裂,视觉差极小,留待专门一轮收敛;**新写 surface 一律 `rounded-lg`**。(2026-07 已收敛掉真正扎眼的 `rounded-2xl` / `rounded-[28px]` / 字面量 `rounded-[10px]` 漂移;新代码不要再写字面量圆角。)

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
- **Don't** add a second border/divider color — every edge in this app is Threshold Line (`#DDEAE6` / `#203538` dark); reach for opacity variants of existing state colors instead.
- **Don't** put small white button labels on raw logo teal (`#0F8F7A`) — use Action Teal (`#087968`) for primary button fills.
- **Don't** add decorative gradients, glassmorphism, or gradient text anywhere — none exist today and none fit a doorstep that's supposed to feel calm and legible, not flashy. (Toasts use `burnt` — the OS-native mechanism per platform: iOS SPIndicator capsule at top, Android system `ToastAndroid` at bottom. No custom toast surface, no blur; see `lib/toast.ts`.)
