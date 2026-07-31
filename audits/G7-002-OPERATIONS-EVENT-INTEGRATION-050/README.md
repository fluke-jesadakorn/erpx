# G7-002/G7-003 Operations source-event integration

## Delivered

Operations now accepts a closed set of source domains (Sales, Procurement,
Inventory, Finance, Maintenance, Support, and Integrations) through typed
immutable source events. Infrastructure derives deterministic Operations event
identity/idempotency, validates holding and scope containment, preserves source
provenance without taking ownership of source transactions, applies ordered
aggregate versions, and exposes bounded source-reference reconciliation pages.
Exception creation and lifecycle reconciliation remain versioned and require
active principal/SLA proofs.

## Independent verification

- `cargo fmt --all` — PASS.
- `cargo test -p erpx-domain` — PASS: 87 tests, including source-event,
  scope, lifecycle, and terminal-state coverage.
- `cargo test -p erpx-infrastructure --lib` — PASS: 61 passed, 4 ignored.
- `cargo check --workspace` — PASS.
- `cargo clippy -p erpx-infrastructure --lib -- -D warnings` — PASS.

## Residuals

Source producers from Sales/Procurement/Inventory/Finance/Maintenance/Support,
durable worker composition, detection scheduling, expired-user reassignment,
manager hierarchy resolution, authenticated Operations routes/UI, and Folio
dashboard reconciliation remain open. Operations does not own source records.

## Security and rollback

Holding references, typed scopes, source identity, correlation, causation, and
idempotency are checked before persistence. A bad event fails closed. Rollback
is limited to the Operations source-reference/event migration and projection
history after verifying no source-owned transaction data is targeted.
