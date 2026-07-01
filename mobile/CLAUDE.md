# CLAUDE.md

## Design Context

This project has `PRODUCT.md` and `DESIGN.md` at the repo root (generated via `/impeccable init`). Read them before any UI/UX work.

- **Register**: `product` — a utility app (cross-network P2P encrypted file transfer), not a marketing surface.
- **North Star**: "The Trusted Doorstep" — control through visibility, not complexity.
- **Personality**: friendly · warm · reassuring. Anti-references: social/entertainment feeds (avatars, likes, timelines) and enterprise SaaS dashboards (data tables, dashboard stacking).
- **Visual system**: unmodified shadcn/ui "New York" blue theme (`src/global.css`), an almost-flat elevation model (visible shadow only on floating surfaces), and a tight 10–15px type scale that carries nearly all hierarchy — see `DESIGN.md` for exact tokens and component specs.
- Fixed: light-mode `--primary-foreground` used to be ~3.5:1 contrast on `--primary` (below WCAG AA 4.5:1). Now unified to the same dark-ink value dark mode already used (~4.9:1) — see `DESIGN.md`'s Unified Ink Rule.
