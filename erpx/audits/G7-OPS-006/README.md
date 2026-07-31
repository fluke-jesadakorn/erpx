# G7-OPS-006 — Tenant-routed Operations projector

## Scope

This packet closes the implementation slice for production execution of the
Operations read projector. It does not claim the G7 product gate.

## Delivered behavior

- `OperationsProjectionRunner` reads immutable Operations events from the
  routed tenant write database and applies them to the routed read database.
- The scheduler executor decodes a closed allow-list of the 17 Operations
  aggregate types and validates event identity, contract version, holding,
  scope, aggregate version, and projection name before writing.
- `SqlxOperationsStore::project_event` supplies contiguous, idempotent
  checkpoints; exact replays are no-ops and gaps fail closed.
- Incremental and rebuild jobs use bounded pages and explicit read-projection
  cleanup. Routed pools are closed on every success and failure path.
- Worker-facing errors are redacted and do not include payloads, tenant
  credentials, database URLs, or aggregate state.
- `apps/worker/src/main.rs` registers the multi-tenant scheduler only when the
  exact handler name is selected, and requires validated server-side tenant
  routing configuration plus scheduler authorization.

## Changed files

- `apps/worker/src/lib.rs`
- `apps/worker/src/main.rs`
- `apps/worker/tests/operations_projection.rs`
- `crates/infrastructure/src/provisioning.rs`

## Evidence

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-worker --lib` — PASS: 41 passed, 1 ignored.
- `cargo test --workspace` — PASS; documented disposable-database tests
  remain ignored by default.
- Disposable four-database PostgreSQL run of
  `operations_projector_executes_through_active_tenant_route` — PASS: 1
  passed. It seeded an Operations event in the tenant write database,
  projected through the active control-plane route, verified exact replay as a
  no-op, and rebuilt the read projection. All four databases were removed
  afterward.
- Dioxus web and desktop checks — PASS.
- `TenantDatabaseProvisioningConfig::from_values` unit coverage rejects zero
  or repeated `{database}` template tokens.

## Security review

The scheduler resolves active tenant bindings from the control-plane registry,
then routes server-side pools. The Operations executor is restricted to the
Operations read projection and fails closed on unknown aggregate types or
identity mismatches. No credentials cross the worker/client boundary.

## Remaining work

Source-context event producers, complete Operations scheduling and exception
producers, authenticated Operations routes/UI, Folio reconciliation, physical
replacement-read activation evidence, and all other ERPX gates remain open.

## Rollback

Unset `ERPX_WORKER_HANDLER=tenant_projection_scheduler` to keep the scheduler
unselected, or revert this packet only after stopping the worker. Folio and
tenant write databases are unaffected by the projector registration.
