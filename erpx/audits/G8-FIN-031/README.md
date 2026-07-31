# G8-FIN-031 — Dedicated Finance close projection

## Delivered behavior

- Added tenant read migration `0031_finance_close_projection.sql` with typed
  close-review/report rows, account summaries, and per-period checkpoints.
- Enabled and forced holding RLS on every projection table.
- Added `SqlxFinanceCloseProjectionStore` source-event paging from the tenant
  write outbox and atomic read-side projection application.
- Added contiguous aggregate-version checks, duplicate delivery no-ops,
  bounded domain decoding, account-row replacement, and rebuild clearing.
- Added `FinanceCloseProjectionRunner` for tenant-routed incremental and full
  rebuild execution.
- Changed the close-read adapter and server helper to query only the tenant
  read database. No tenant write pool is opened for the close read path.

## Evidence

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-infrastructure finance_close_projection -- --nocapture` —
  PASS; 2 tests.
- `cargo test -p erpx-worker finance_close_projection -- --nocapture` — PASS;
  2 tests.
- `cargo test -p erpx-migration -- --nocapture` — PASS; 6 tests.
- `cargo clippy -p erpx-infrastructure -p erpx-worker -p erpx-server
  --all-targets -- -D warnings` — PASS.
- Migration author independently ran a temporary PostgreSQL syntax transaction
  and rolled it back — PASS.

## Open boundary and rollback

Live disposable tenant database projection/rebuild evidence, durable scheduler
registration, Finance UI, and full Finance reconciliation remain open. Rollback
rebuilds or omits the disposable tenant read database migration and removes the
additive projector/adapter/worker wiring; tenant write data and Folio remain
unchanged.
