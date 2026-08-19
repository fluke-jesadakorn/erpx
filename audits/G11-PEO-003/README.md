# G11-PEO-003 — HR/payroll read projection schema and worker

## Delivered behavior

- Added holding read migration 0032 with typed employee, leave, expense,
  payroll run/line, and expired-role reassignment tables.
- Added checkpoints and event receipts with source versions/event IDs.
- Enabled and forced holding RLS on all eight tables.
- Added `SqlxHrProjectionStore` for ordered write-outbox source paging and
  typed read-side upserts across all five HR aggregate types.
- Added `HrProjectionRunner` for incremental projection and full disposable
  read-model rebuilds. Payroll lines are replaced atomically per run.
- No raw source JSON or command context is persisted in the read model.

## Evidence

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-infrastructure hr_projection -- --nocapture` — PASS; 2
  tests.
- `cargo test -p erpx-worker hr_projection -- --nocapture` — PASS; 2 tests.
- `cargo clippy -p erpx-infrastructure -p erpx-worker --all-targets --
  -D warnings` — PASS.
- `cargo test -p erpx-migration -- --nocapture` — PASS; 6 tests.
- Migration author independently applied the schema in a disposable
  PostgreSQL transaction, confirmed eight tables had forced RLS, and rolled it
  back — PASS.

## Open boundary and rollback

HR read transport, UI, durable scheduler registration, live projector replay,
and end-to-end HR reconciliation remain open. Rollback removes migration 0032
and additive projector/worker code; holding-write source state and Folio remain
unchanged.
