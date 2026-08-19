# G6-PRV-013 — control-read projection status storage and adapter

## Delivered behavior

Added control-read migration `0004_holding_projection_status.sql` for a
rebuildable, metadata-only projection of durable holding scheduler status. The
table is physically separate from the control-write scheduler rows and has
bounded opaque holding/holding/projection/job references, closed kind/state
values, bounded attempts/errors, checkpoint/version timestamps, and
deterministic keyset indexes. It contains no job content, claim identity,
worker identity, credentials, database URLs, or storage references.

Added `HoldingProjectionStatusReadRepository`, which reads only the control-read
pool, validates filters and cursor/filter binding, enforces the page limit, and
returns the existing safe status DTO. The Platform Operations status RPC now
uses this read-side repository rather than the control-write scheduler pool.

## Files changed

- `crates/migration/migrations_read/0004_holding_projection_status.sql`
- `crates/migration/src/lib.rs`
- `crates/infrastructure/src/holding_projection_scheduler.rs`
- `crates/infrastructure/src/lib.rs`
- `crates/infrastructure/tests/holding_projection_status.rs`
- `apps/server/src/main.rs`
- `audits/G6-PRV-013/README.md`

No holding migration, Folio route, environment file, or runtime service state
changed.

## Verification

From `erpx/`:

```text
cargo fmt --all — PASS
cargo test -p erpx-migration — PASS: 6 tests; migration 0004 embedded and
  sensitive-column assertions passed
cargo test -p erpx-infrastructure --test holding_projection_status — PASS: 5 tests
cargo check -p erpx-server — PASS
cargo test -p erpx-server --bin erpx-server — PASS: 61 tests
```

The full integration-wave verification is recorded in the execution ledger.

## Unfinished work and rollback

Durable scheduler-to-read projection publishing, status freshness producers,
projection rebuild/action RPCs, and the complete Platform Operations UI/data
journey remain open. Rollback removes the additive read migration, read-side
repository/export, server pool selection, tests, and this audit; because the
table is rebuildable, no write-side rollback is required.
