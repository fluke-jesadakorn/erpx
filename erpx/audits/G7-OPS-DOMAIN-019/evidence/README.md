# G7 Operations domain packet evidence

This packet establishes the transport-neutral organization and Operations
aggregate contract plus a tenant-bound SQLx write/read persistence slice. It
is intentionally not the G7 completion gate: worker composition, exception
workers, branch queues, Folio/ERPX reconciliation, authenticated routes, and
the full Operations UI remain open.

## Delivered behavior

- Holding, company, headquarters, branch, department, and team aggregates
  validate explicit holding/company/branch scope containment.
- OperationalPlan, OperationalCommitment, FulfillmentPlan, CapacityPlan,
  ExceptionCase, WorkQueuePolicy, OperatingCalendar, Shift, ShiftHandover,
  StandardOperatingProcedure, and DispatchBatch contracts are defined.
- Operations stores only immutable source transaction references and rejects
  references whose source domain is Operations.
- Lifecycle transitions reject skipped transitions and terminal-state reuse.
- Event envelopes carry aggregate identity/version, tenant scope, actor,
  correlation, causation, idempotency, and canonical payload metadata.
- Unordered reference collections have deterministic canonical forms.
- `SqlxOperationsStore` persists snapshots and source references atomically,
  checks expected versions, and rejects cross-holding scopes.
- Operations event replay is idempotent by event ID/idempotency key; read
  projection application is ordered and checkpointed.
- A tenant-routed worker can read a bounded, ordered event page with an
  exclusive aggregate/version cursor, and can prepare a projection-specific,
  holding-scoped read-model rebuild without touching write-side events or
  other projections.
- Tenant write/read migrations create forced-RLS Operations tables without
  changing the control plane.

## Independent verification

Run from `erpx/`:

| Command | Result |
|---|---|
| `cargo fmt --all --check` | PASS |
| `cargo clippy -p erpx-domain --all-targets -- -D warnings` | PASS |
| `cargo test -p erpx-domain` | PASS; 55 tests |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS |
| `cargo test --workspace` | PASS; all non-disposable tests passed |
| `cargo check -p erpx-console --no-default-features --features web` | PASS |
| `cargo check -p erpx-console --no-default-features --features desktop` | PASS |
| `cargo test -p erpx-infrastructure --test operations_persistence operations_persistence_write_read_boundary_is_versioned_and_idempotent -- --ignored --nocapture` | PASS; 1 disposable PostgreSQL write/read test; both databases dropped afterward |

## Security and ownership review

- Aggregate scope rejects platform and unrestricted subtree scopes.
- Branch/company containment is checked before accepting nested planning
  scopes.
- Operations source ownership is represented by immutable IDs and event IDs;
  no source transaction rows are copied into this contract.
- Aggregate mutation still requires the shared authoritative authorization
  boundary when a repository/transport command is added.

## Open integration work

- Worker composition and outbox/projector consumption from the tenant event
  stream; the page/rebuild primitives are present but not yet wired into the
  production worker loop.
- Exception SLA, escalation, expired-user reassignment, and manager queues.
- Operations routes and reconciliation against Folio `/operations` totals.

## Rollback

This packet is additive and has no database migration or runtime command-owner
change. Roll back only the Operations module/export and this evidence file if
the packet is rejected; preserve all unrelated user changes.
