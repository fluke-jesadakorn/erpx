# G11-AST-003-057 — Asset/Maintenance holding projector seam

## Scope

Main-agent integration of the bounded Asset/Maintenance source-event page and
holding-routed worker projector runner. The runner reads only the holding write
outbox, applies idempotent read projections, and supports clear-and-replay
rebuilds through the routed write/read database pair. Holding and holding
references, cursor identity, page size, and worker errors are bounded.

## Evidence

- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-infrastructure` — PASS.
- `cargo test -p erpx-infrastructure --lib` — PASS: 75 passed, 4 documented
  disposable-PostgreSQL tests ignored.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo check -p erpx-worker` — PASS.
- `cargo test -p erpx-worker --lib` — PASS: 19 passed, 1 documented
  disposable-PostgreSQL test ignored.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.

## Security and rollback

The source page is holding-scoped and uses a stable aggregate/type/version
cursor; the worker emits bounded redacted errors and never exposes database
configuration. The read projection remains disposable. Rollback is removal of
the runner registration and evidence entry while retaining authoritative
Asset/Maintenance write state and migrations.

## Remaining work

Durable per-holding scheduling/claiming, transport, GL posting, Operations
consumption, HR/Legal integration, mobile UI, and end-to-end reconciliation
remain open. This packet does not close the G11 gate.
