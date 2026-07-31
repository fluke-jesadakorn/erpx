# G9-003 Sales tenant projector seam

## Scope

The Sales write repository exposes bounded immutable source-event pages in
stable aggregate/type/version order. `SalesProjectionRunner` routes a
holding's write and read databases, applies each event through the typed Sales
projection boundary, and supports a clear-and-replay rebuild. Projection
errors are redacted at the worker boundary. Request validation rejects unsafe
tenant references and unbounded pages.

## Changed files

- `crates/infrastructure/src/sales_persistence.rs`
- `crates/infrastructure/src/lib.rs`
- `apps/worker/src/lib.rs`
- `audits/G9-SALES-003-056/README.md`
- `MASTER_PLAN.md`
- `EXECUTION_LEDGER.md`

## Independent verification

Run from `erpx/`:

- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-worker` — PASS.
- `cargo test -p erpx-worker --lib` — PASS: 18 passed, 1 documented
  disposable-PostgreSQL test ignored; includes the Sales request-boundary
  test.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.
- `cargo check -p erpx-infrastructure` — PASS.
- `cargo test -p erpx-infrastructure --lib` — PASS: 69 passed, 4 documented
  disposable-PostgreSQL tests ignored.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.

## Open boundaries

This is a tenant projection seam, not full production worker composition. It
does not yet claim durable per-tenant event claiming, scheduling, transport,
or complete Sales workflow reconciliation.

## Rollback

Remove the runner composition and exports, then retain the Sales write rows
and outbox for controlled replay. No existing migration is removed by this
packet.
