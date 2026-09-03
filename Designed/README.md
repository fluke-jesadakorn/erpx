# Designed — Clickable Mockup (single source of UI truth)

> No `.pen` files (removed 2026-09-03: `MainDesign.pen` 15M + `ERPX-System.pen` + `images/` 16M — recover via `git show <sha> --stat`, see log). `Designed/AGENTS.md` canvas rules retired with the pens.

## Mockup — `mockup/index.html`

- **One self-contained file** (needs internet: Tailwind Play CDN + Lucide CDN; no build, no deps): open via any static server, e.g. `python3 -m http.server -d Designed/mockup` → `http://localhost:8000/index.html#/feed`.
- **Stack:** Tailwind utilities (`tailwind.config` mapped to tokens) + genuine shadcn CSS-variable theme (`:root`/`.dark`, primary tuned to brand `#6758e5`) + Lucide icons (`lucide.createIcons()` re-run per view). Real shadcn *components* are React-only — this is the faithful static equivalent; port to `src/app/` when promoting to production.
- **From `@docs`:** 13 groups / 31 contexts (`docs/plan/MAP.md`), metrics + sample rows per context, feed spec newest-first + role lanes (`docs/plan/04-social-paradigm.md`, ADR-021), typed `@` chips + `#` tags + follows, activity rail + AI ghost card per record (`_shared-contract.md`), spine refs (`crm/opportunity-2026-011`, `projects/proj-2026-001`).
- **Clickable:** hash routing `#/feed #/today #/tags #/journeys #/ctx/:ctx #/rec/:ctx/:id`, notifications dropdown, search filter per view, working composer (posts to feed), follow/like toggles (reactions signal-only with toast), proof-gated Apply demo.
- **Deep per context (from `docs/plan` 02/03 + `_shared-contract` + `_journeys`):** lifecycle stepper + state-machine actions (proof-gated where docs require) on all 31 contexts, kind-validated field forms (OwnerPublished: sales/procurement), typed reference edges with roles + reorder, AI suggestion lifecycle (Ready→Applied/Rejected, drift→Conflicted→Re-request), append-only audit per record, backlinks, @ picker in every input, kanban (talent/support with live SLA clocks), WBS/org trees, 4 clickable journeys, Today action lanes with inline Approve/Reject, per-context New-record forms. All in-memory demo state.
- **Look:** flat editorial — warm paper + ink + single persimmon accent (`#B3402A`), hairline borders, solid buttons, flat avatar palette, zero gradients/glows. Dark mode = warm charcoal. Same shadcn token plumbing, retuned values.
- **Verify:** serve + screenshot `#/feed`, `#/ctx/sales`, `#/rec/sales/QT-2026-042` (done 2026-09-03, zero console errors).

## Recovery

Pens + images recoverable from git history (`git log -- Designed/MainDesign.pen`). Rust-era canvas on `backup/rust-2026-09-02`.
