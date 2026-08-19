# G7-001 — Organization hierarchy command/read boundary

## Delivered

- Added a holding-local `OrganizationService` with authenticated Tonic and
  gRPC-Web save/list endpoints for Holding, Company, Headquarters, Branch,
  Department, and Team aggregates.
- Commands route through the registered holding's physical write/read pools,
  require active `organization.admin`/`organization.read` permissions, reject
  platform scope, verify holding identity against the opaque holding route,
  and enforce requested-scope containment.
- Typed Operations snapshots/events are written atomically on the write side;
  the matching `operations_read_model` row/checkpoint is projected on the
  separate read side.
- Deterministic event IDs and persisted idempotency records make retries
  harmless, while optimistic versions and lifecycle transitions remain
  authoritative on the write side.
- Read cursors are bounded and opaque. The adapter additionally rejects any
  row whose holding reference does not match the routed store, even if a
  privileged database connection bypasses PostgreSQL RLS.

## Evidence

- `cargo check -p erpx-infrastructure` — PASS.
- `cargo check -p erpx-server` — PASS.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-infrastructure --test organization` — PASS; one unit
  test passed and the database test was ignored without URLs.
- Disposable two-database proof
  `organization_write_read_projection_is_holding_bound_idempotent_and_versioned`
  — PASS; all six node kinds persisted and projected, replay returned
  `applied=false` at version one, and cross-holding reads failed closed. The
  temporary databases were removed by the test command cleanup trap.

## Remaining scope

Reporting lines, delegated administration, composite organization constraints,
full organization read filtering, holding-owner activation inputs for legal
companies/branches, and the broader G7 Operations command/read surfaces remain
open. This packet does not claim the G7 or final ERPX gates.
