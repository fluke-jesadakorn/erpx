# G7-002 — Operations query boundary and reconciliation

## Delivered

- Added `OperationsService.GetOperationsSummary` and
  `OperationsService.ListOperationsExceptions` to the shared protobuf contract.
- Added authenticated Tonic/gRPC-Web server handlers with required
  `operations.read` authorization and contained holding scope.
- Resolved the opaque holding registry reference before routing; inactive holdings,
  mismatched holding scopes, missing routes, and invalid cursors fail closed.
- Routed each request to the physical holding write/read database pair. The
  summary reads write-side event/source-reference counts and read-side
  projection/checkpoint counts without making Operations authoritative for
  Sales, Procurement, Inventory, Finance, Maintenance, Support, or Integrations.
- Added a bounded, deterministic active-exception queue ordered by projection
  update time and aggregate ID. The transport omits exception source payloads
  and exposes only safe operational fields.
- Added a bounded exception page-size policy, opaque cursor encoding, safe
  persistence error mapping, and correlation propagation.

## Evidence

- `cargo fmt --all --check` — PASS.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo clippy -p erpx-server --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-infrastructure --test operations_persistence` — PASS;
  3 unit tests passed and 3 database tests remained ignored without URLs.
- Disposable two-database test
  `operations_persistence_write_read_boundary_is_versioned_and_idempotent` —
  PASS. The test exercised the new summary across separate write/read
  databases and then rebuilt the read projection.
- Full post-integration wave — PASS: workspace clippy, workspace tests,
  Dioxus web/desktop checks, Folio lint/typecheck, `git diff --check`,
  PostgreSQL readiness, Folio `/operations` HTTP 307, and MinIO health HTTP
  200. Folio dev server remained active on port 3004.

## Reconciliation contract

The summary is intentionally a reconciliation surface, not a replacement for
the Folio dashboard source queries. `event_count` and source-domain reference
counts come from the Operations write-side event/provenance tables, while
`read_row_count`, `exception_count`, and projection checkpoints come from the
separate disposable read database. A future source-context adapter will map
Sales, Procurement, Inventory, Finance, Maintenance, Support, and Integration
facts into this projection and compare them to Folio during G13 migration.

## Remaining work

- Add source-context adapters and deterministic Folio/ERPX KPI comparison
  fixtures; no claim is made that the legacy dashboard totals reconcile yet.
- Add the holding Dioxus Operations read surface and all branch execution pages.
- Complete organization reporting lines, delegated administration, composite
  constraints, exception lifecycle commands/workers, and G7/G14 gates.

The persistent ERPX goal remains active.
