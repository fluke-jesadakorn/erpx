# Plan — Archive (canonical for all pruned `Plan`/`docs` history)

> **Single canonical — 2026-09-02.** All banners `Pruned on development 2026-09-02` (`docs/plan/LEGACY.md`, `docs/README.md`, `docs/contracts/README.md`, `docs/schema/README.md`, `CUTOVER.md:2026-09-02`) point here. Living spec is `docs/plan/` (31 contexts) + `docs/TOC.md` + `docs/plan/MAP.md` (31 rows).

## What was pruned on `development` (`99a66de7` A,B)

- `Plan.legacy-2026-08-29/` — 43 Markdown files (aspirational FSDs: `FSD-001..006`, portfolios 01..15, platform docs)
- `Plan.legacy-codebase-2026-08-29/` — 40 Markdown files (technical snapshot: `01-architecture..06-operations`, 31 `contexts/<name>.md`, `traceability.md`, `glossary.md`)
- `docs/contracts/proto/**` — 37 `*.proto` files from `crates/proto/proto/erpx/**` + `contracts/descriptor/erpx_descriptor_set.sha256` + prior descriptor (860K bin + provenance)
- `docs/schema/migrations*` — 28 SQLx embedded migrations (control-write/read, holding-write/read, generation-v2) from `crates/migration/`

Briefly copied to `docs/` at `f97b74d6` (`backup/rust-2026-09-02`), then removed from `development` at `99a66de7`.

## Recovery (3 hashes — single source)

- Archive point: `f97b74d6` (`chore: backup Rust monorepo before Next.js cutover`)
- Tag: `rust-archive-2026-09-02` at `f97b74d6`
- Baseline: `cc0887dd3f16fe8dbdc827c1655750d9fafd91d2` (`checkpoint: preserve pre-ERPX cutover bridge state`)

```bash
git checkout backup/rust-2026-09-02
ls Plan.legacy-2026-08-29/ | wc -l        # 43
ls Plan.legacy-codebase-2026-08-29/ | wc -l # 40
ls docs/contracts/proto/erpx/*/service.proto | wc -l # 37
ls docs/schema/migrations*/000*.sql | wc -l   # 28
git show rust-archive-2026-09-02 --stat   # f97b74d6
git show cc0887d --stat
# Restore to development (if needed):
git checkout development -- docs/plan/LEGACY.md  # stub
git checkout backup/rust-2026-09-02 -- Plan.legacy-2026-08-29/ Plan.legacy-codebase-2026-08-29/ docs/contracts/ docs/schema/
```

## Living replacement

- `docs/plan/` 31 contexts (13 groups, 26-35L light + 60-65L deep trimmed via `_shared-contract.md` 109L, `MAP.md` 31 rows, 13 group `README.md`)
- `docs/TOC.md` (single hub) → `docs/plan/MAP.md` → `docs/plan/01-vision-and-scope.md` (spine) → `src/app/` + `docs/contracts/next/route-table.md` (living)
- `docs/plan/LEGACY.md:1` (18L stub) + `docs/README.md:1` (8L) + `docs/contracts/README.md:1` (5L) + `docs/schema/README.md:1` (5L) — each 4-line pointer here (not 25L inventory)
