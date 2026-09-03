# Designed — Clickable Mockup (single source of UI truth)

> No `.pen` files (removed 2026-09-03: `MainDesign.pen` 15M + `ERPX-System.pen` + `images/` 16M — recover via `git show <sha> --stat`, see log). `Designed/AGENTS.md` canvas rules retired with the pens.

## Mockup — `mockup/index.html`

- **One self-contained file** (inline CSS + JS, no build, no deps): open via any static server, e.g. `python3 -m http.server -d Designed/mockup` → `http://localhost:8000/index.html#/feed`.
- **From `@docs`:** 13 groups / 31 contexts (`docs/plan/MAP.md`), metrics + sample rows per context, feed spec newest-first + role lanes (`docs/plan/04-social-paradigm.md`, ADR-021), typed `@` chips + `#` tags + follows, activity rail + AI ghost card per record (`_shared-contract.md`), spine refs (`crm/opportunity-2026-011`, `projects/proj-2026-001`).
- **Clickable:** hash routing `#/feed #/today #/tags #/ctx/:ctx #/rec/:ctx/:id`, notifications dropdown, search filter per view, working composer (posts to feed), follow/like toggles (reactions signal-only with toast), proof-gated Apply demo.
- **Style:** shadcn tokens (`--primary:#6758E5`, radius 10), Inter/system + Thai fallback, inline SVG icons (no emoji), responsive (rail hides <1100px, sidebar drawer <820px).
- **Verify:** serve + screenshot `#/feed`, `#/ctx/sales`, `#/rec/sales/QT-2026-042` (done 2026-09-03, zero console errors).

## Recovery

Pens + images recoverable from git history (`git log -- Designed/MainDesign.pen`). Rust-era canvas on `backup/rust-2026-09-02`.
