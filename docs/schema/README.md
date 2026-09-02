# docs/schema — Archived Migrations Inventory (pruned on development)

> **Pruned from `development` on 2026-09-02 (`99a66de7`).** The 28 SQLx migration files listed below are **reference-only** — Rust/SQLx era archived on `backup/rust-2026-09-02` (`rust-archive-2026-09-02`, `f97b74d6`). Living migrations on this branch will be Prisma/Drizzle (TBD) — see `CONTRACT_POLICY.md` and `ADR-020`.

## What was there

- `schema/migration_lib.rs` + `schema/schema_manifest.rs`
- `schema/migrations/0001_baseline.sql` … `0008_social_feed.sql` (8 files) — control DB
- `schema/migrations_holding_read/0001_baseline.sql` … `0005_procurement_workflow.sql` (5 files)
- `schema/migrations_holding_write/0001_baseline.sql` … `0012_social_feed.sql` (12 files)
- `schema/migrations_read/0001_baseline.sql` … `0003_global_collaboration_mention_inbox.sql` (3 files)
- Total: 28 SQL files (generation-v2, embedded SQLx)

## Recovery

```bash
git checkout backup/rust-2026-09-02
ls docs/schema/migrations*/000*.sql | wc -l   # 28
ls docs/schema/
git show rust-archive-2026-09-02 --stat | grep schema
```

## Living replacement

- `prisma/` or `drizzle/` (TBD) at repo root — new migrations for Next.js, referenced from `CONTRACT_POLICY.md`
