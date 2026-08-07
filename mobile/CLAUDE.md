# CLAUDE.md

## Design Context

This project has `PRODUCT.md` and `DESIGN.md` at the repo root (generated via `/impeccable init`). Read them before any UI/UX work.

- **Register**: `product` — a utility app (cross-network P2P encrypted file transfer), not a marketing surface.
- **North Star**: "The Trusted Doorstep" — control through visibility, not complexity.
- **Personality**: friendly · warm · reassuring. Anti-references: social/entertainment feeds (avatars, likes, timelines) and enterprise SaaS dashboards (data tables, dashboard stacking).
- **Visual system**: shadcn/ui "New York" shape language on the **Harbor Teal** brand palette
  (`src/global.css`), an almost-flat elevation model (visible shadow only on floating surfaces),
  and a tight 11–15px type scale that carries nearly all hierarchy — see the root `DESIGN.md`
  for exact tokens and component specs.
- **The brand teal has two forms and they are not interchangeable** (root `DESIGN.md`,
  Brand Fidelity Rule): `--primary` is the **fill** (`#108F7A` light / `#14A38C` dark),
  `--primary-ink` is the **text/icon** form (`#087968` / `#5EE0C8`). Mobile used to set
  `--primary` to the *text* value, which made every primary button a shade darker here than on
  desktop; fixed 2026-08-04 by aligning both to the desktop `oklch` anchors.
- Fills carry **deep ink** text (`--primary-foreground`, ~5:1), never white. This file claimed
  that was already the case while the code still shipped white — the code is now what matches.
- NativeWind needs raw HSL triples (`hsl(var(--x))`), so mobile **cannot** share the desktop/web
  `oklch` expressions verbatim; the values are the same colors, converted. Changing one side
  means converting, not copying.
