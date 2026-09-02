# Plan — Legacy Baselines (archived)

> **Pruned from `development` on 2026-09-02 (`99a66de7`).** These baselines are not on this branch anymore — history remains on `backup/rust-2026-09-02`.

- **Pruned directories:**
  - `Plan.legacy-2026-08-29/` — 43 Markdown files (aspirational FSDs) — `FSD-001`..`FSD-006`, portfolios 01..15, platform docs, acceptance scenarios.
  - `Plan.legacy-codebase-2026-08-29/` — 40 Markdown files (technical snapshot of the Rust codebase) — `01-architecture.md`..`06-operations-and-deployment.md`, 31 `contexts/<name>.md`, `traceability.md`, `glossary.md`.
- **Also pruned (A,B):** `docs/contracts/proto/**` (37 `*.proto` + descriptors) and `docs/schema/migrations*` (28 SQL files) — briefly copied to `docs/` at `f97b74d6`, removed on `99a66de7`.
- **Recovery:**
  ```bash
  git checkout backup/rust-2026-09-02
  ls Plan.legacy-2026-08-29/
  ls Plan.legacy-codebase-2026-08-29/
  ls docs/contracts/proto/  # on that branch — pruned here
  git show rust-archive-2026-09-02 --stat   # f97b74d6
  git show cc0887d --stat
  ```
- **Living replacement:** `Plan/` (68 files) is the only living business plan on this branch — see `Plan/README.md` and `docs/TOC.md`. For deep technical history see `adr/` (20 decisions, ADR-020 is Next.js) and `docs/archive/delivery-2026-08/` (768-line Rust DDD snapshot, archived).
