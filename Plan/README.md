# ERPX — Open-Source Business Plans

> **Code is truth.** Every capability below traces to the Next.js implementation (`src/app/`) and to the archived Rust contracts previously at `crates/proto` / `crates/contexts` (archived on `backup/rust-2026-09-02` — see `CUTOVER.md`). Previous baselines `Plan.legacy-2026-08-29/` and `Plan.legacy-codebase-2026-08-29/` were pruned from this branch on 2026-09-02 (A) — history remains on backup.

## What ERPX is

Web-first Next.js 16 ERP on this branch (App Router `src/app/` on `http://localhost:3000`, route handlers). Rust/Dioxus/Tonic (`:3004`/`:50051`/`:50052`, DDD + Clean + Hex/CQRS) is archived on `backup/rust-2026-09-02` — see `CUTOVER.md` and `docs/README.md`. See `01-vision-and-scope.md`.

## Who it's for

Everyone — open-source. Self-hosters, contributors, enterprise evaluators, and operators. No hidden commercial gate: the code in this repo is the product.

## How to use this Plan

- `01-vision-and-scope.md` — vision, scope, licensing, deployment tiers, contribution model, and the **universal `@`-mention** contract.
- `02-capabilities/<group>/<context>.md` — 31 lightweight one-pagers (Q5=A, <80 lines each) grouped by value stream (Q8=B). Each links to proto + domain + data + verification.
- `CONTEXT.md` (repo root) — code-derived glossary, no implementation.
- `Plan/glossary.md` — pointer to `CONTEXT.md`.
- `Plan/traceability.md` — RPC/route/table → code → spec matrix.

## Quick links

- Verify: `npm run lint` + `npm run build` (Next.js; Rust gate `cargo xtask verify` is archived on backup)
- ADRs: `adr/` (19 decisions — see `adr/ADR-020` for Next.js stack; historical via backup)
- Recovery: `backup/rust-2026-09-02` + tag `rust-archive-2026-09-02` + baseline `cc0887d` (see `CUTOVER.md`)

## Edit rule (Q18=A)

PRs welcome. Any `Plan/` edit must update `traceability.md` and reference the code SHA it describes. Docs = code.
