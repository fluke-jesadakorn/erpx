# G11-AST-002 Asset/Maintenance SQLx persistence

## Scope

This packet adds holding-routed SQLx persistence for Asset and
MaintenanceWorkOrder aggregates. The write database is authoritative and the
read database is disposable. Mutations validate domain state, enforce
holding-reference RLS, optimistic contiguous versions, and request-bound
idempotency, then atomically write aggregate state, shared CQRS snapshot,
outbox event, and receipt. Custody history is normalized as append-only source
state and read projection rows. Operations capacity-impact handoffs are
persisted as source-owned immutable handoffs with their own outbox event.

## Changed files

- `crates/infrastructure/src/asset_maintenance_persistence.rs`
- `crates/infrastructure/src/lib.rs`
- `crates/migration/migrations_holding_write/0021_asset_maintenance.sql`
- `crates/migration/migrations_holding_read/0021_asset_maintenance.sql`
- `crates/migration/src/lib.rs`
- `MASTER_PLAN.md`
- `EXECUTION_LEDGER.md`

## Independent verification

Run from `erpx/`:

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-infrastructure --lib` — PASS: 75 passed, 4 documented
  disposable-PostgreSQL tests ignored; includes Asset/Maintenance persistence
  validation tests.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-migration` — PASS: 6 passed; assertions cover migration
  0021, forced RLS, custody immutability, and read checkpoints.
- `cargo check -p erpx-infrastructure` — PASS.

## Open boundaries

Asset/Maintenance transport, worker event consumption, GL posting,
Operations projection consumption, HR/Legal integration, and full lifecycle
reconciliation remain open. This packet does not claim completion of G11.

## Security concerns

Holding RLS is forced on all source, custody, handoff, read, and checkpoint
tables. Secrets are not modeled. Persistence callers must still perform
authoritative RBAC, entitlement, billing, SoD, audit, and actor checks before
calling the repository.

## Rollback

Before deployment, remove the repository export and omit migration 0021. After
deployment, stop routing Asset/Maintenance commands to this adapter and retain
the write rows for controlled replay; read rows are rebuildable.
