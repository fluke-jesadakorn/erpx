# ERPX — Open-Source Business Plans

> **Code is truth.** Every capability traces to `src/app/` (Next.js, `http://localhost:3000`) + archived Rust `crates/proto` / `crates/contexts` on `backup/rust-2026-09-02` (`CUTOVER.md` 2026-09-02). History: `docs/plan/ARCHIVE.md` (canonical) + `docs/plan/LEGACY.md` stub.

## What ERPX is

Web-first Next.js 16 ERP (App Router, route handlers). Rust/Dioxus/Tonic (`:3004/:50051/:50052`) archived on `backup/rust-2026-09-02` — see `docs/TOC.md` (hub) + `01-vision-and-scope.md`.

## Who it's for

Everyone — open-source, self-hosters, contributors, enterprise. No commercial gate — this repo is the product.

## How to use this Plan

- `docs/plan/MAP.md` — flat index 31 contexts (find `sales` without remembering `03`)
- `02-capabilities/<group>/README.md` — 13 group indexes (e.g., `03 Finance/Sales/POS`)
- `02-capabilities/<group>/<context>.md` — 31 one-pagers (26-35L, domain invariants + Data owned)
- `03-cross-cutting/@-and-ai/<ctx>.md` — 31 deep (60-65L delta) + `_shared-contract.md` (109L single source for @/AI)
- `01-vision-and-scope.md` — vision, tiers, universal `@`-mention contract + spine
- `CONTEXT.md` + `docs/plan/glossary.md` — glossary, `traceability.md` → code/spec matrix (archived 552 rows in `<details>` + living `docs/contracts/next/route-table.md`)

## Quick links

- Verify: `npm run lint` + `npm run build` (Next.js; `cargo xtask verify` archived on backup)
- ADRs: `docs/adr/` 20 decisions (`docs/adr/README.md` index, `ADR-020` living)
- Recovery: `backup/rust-2026-09-02` + `rust-archive-2026-09-02` + `cc0887d` (see `CUTOVER.md` + `docs/plan/ARCHIVE.md`)

## Edit rule (Q18=A)

PRs welcome. Any `docs/plan/` edit must update `traceability.md` + code SHA. Docs = code.
