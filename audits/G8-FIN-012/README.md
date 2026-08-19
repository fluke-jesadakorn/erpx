# G8-FIN-012 — Finance banking read projection and worker seam

## Delivered behavior

The holding-read migration `0029_finance_banking_projection.sql` adds replaceable
banking read rows and per-aggregate projection checkpoints with forced RLS and
holding-scoped indexes. `SqlxFinanceBankingStore` now validates the allow-listed
event/aggregate mapping, decodes and validates typed account/import/reconciliation
payloads, applies contiguous events idempotently, reads only projected rows, and
clears the projection for a rebuild. The write database remains the source-event
authority.

`FinanceBankingProjectionRunner` routes a holding through the existing holding
router, consumes bounded write-side source pages, writes only to the routed read
database, supports rebuild/replay, closes both routes on success and failure, and
returns redacted worker errors. The worker does not construct holding credentials
or choose database URLs.

## Files changed

- `crates/infrastructure/src/finance_banking_persistence.rs`
- `crates/infrastructure/src/lib.rs`
- `crates/infrastructure/tests/finance_banking_persistence.rs`
- `crates/migration/src/lib.rs`
- `crates/migration/migrations_holding_read/0029_finance_banking_projection.sql`
- `apps/worker/src/finance_banking_projection.rs`
- `apps/worker/src/lib.rs`
- `erpx/audits/G8-FIN-012/README.md`

## Verification

Run from `erpx/`:

```text
cargo fmt --all
cargo check -p erpx-worker
cargo clippy -p erpx-worker --all-targets -- -D warnings
cargo test -p erpx-infrastructure --test finance_banking_persistence
cargo test -p erpx-worker
```

Results: all commands passed. The focused infrastructure test had 2 active
tests pass and 2 ignored disposable-database tests. The worker suite had 43
active tests pass and 1 ignored integration test.

Independent live evidence used two explicitly named disposable PostgreSQL
databases, applied holding-write/read migrations, wrote three banking source
events, projected them, verified idempotent replay, cleared the read model, and
replayed the source page successfully. The live test passed and both databases
were dropped; a follow-up catalog query found no matching databases.

## Security and unfinished work

New read tables use forced RLS and every read/projection transaction sets the
routed holding setting. Read payloads are validated typed domain data and do not
contain bank credentials or payment instruments. Event types and aggregate types
are allow-listed, source versions must be contiguous, and the read path never
falls back to write tables.

Provider adapters, account activation/maintenance commands, bank-to-GL matching,
Finance banking gRPC transport, scheduler registration, and the remaining G8
workflows are still open.

## Rollback

Do not reset unrelated work. Before production use, remove the isolated worker
module/exports and holding-read migration only through a reviewed migration plan.
For an already migrated holding, rebuild or retire the replaceable read tables
after dependent projection retention is handled; never drop holding data ad hoc.
