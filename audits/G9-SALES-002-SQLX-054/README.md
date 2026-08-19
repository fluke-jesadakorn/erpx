# G9-002 Sales SQLx persistence — independently reviewed handoff

## Scope

This packet adds the holding-routed Sales write/read persistence seam for the
Sales contract slice. The write database is authoritative; the read database
contains a disposable projection with source versions. A validated mutation
uses one scoped transaction for the Sales aggregate row, shared aggregate
snapshot, outbox event, and idempotency receipt. Replays return the original
receipt, payload changes are rejected, and optimistic versions reject stale
commands. The read seam applies typed outbox payloads with contiguous
aggregate-version checks, idempotent replay behavior, checkpoints, and a
transactional clear operation for rebuilds. Both migration files force
holding-reference RLS.

The write-side source stream is exposed through a bounded, stable
aggregate/type/version page cursor so a holding worker can consume immutable
Sales events without using the read database as command authority.

## Changed files

- `crates/infrastructure/src/sales_persistence.rs`
- `crates/infrastructure/src/lib.rs`
- `crates/migration/migrations_holding_write/0019_sales.sql`
- `crates/migration/migrations_holding_read/0019_sales.sql`
- `crates/migration/src/lib.rs`
- `MASTER_PLAN.md`
- `EXECUTION_LEDGER.md`

## Independent verification

Run from `erpx/`:

- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-infrastructure` — PASS.
- `cargo test -p erpx-infrastructure --lib` — PASS: 69 passed, 4 documented
  disposable-PostgreSQL tests ignored.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-migration` — PASS: 6 passed; migration assertions
  include holding write/read version 19, Sales tables, source-version reads,
  and forced RLS.

## Open boundaries

Sales transport composition, event projector implementation, source event
production, Procurement/Inventory/Logistics, Finance handoff, and the full
Lead-to-Cash reconciliation gate remain open. The persistence adapter is not
claimed as completion of G9.

## Rollback

Before deployment, omit migration 0019 and do not compose
`SqlxSalesStore`. After deployment, stop routing Sales commands to this store,
retain the write rows for a controlled rollback, and rebuild/remove the
replaceable read projection only through the holding migration/rebuild process.
