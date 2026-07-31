# G14-QA-062 follow-up — scheduler authorization and multi-tenant routing

Date: 2026-07-25  
Owner: Main agent  
Status: integrated; independent re-audit still required

This follow-up records changes made after the read-only `G14-QA-062` audit
completed. The original audit remains historical evidence for the state it
inspected and is not rewritten.

## Integrated corrections

- `TenantProjectionSchedulerRepository` now requires a bounded
  `ProjectionSchedulerAuthorization` at construction.
- Every schedule, claim, renewal, completion, failure, and stale-reclaim path
  verifies active registry state and exact holding binding, then evaluates the
  actor's `tenant.projection.execute` permission through authoritative
  control-plane role assignments for the UUID holding scope.
- Accepted scheduler operations write bounded control-plane audit events with
  actor, operation, tenant/holding, permission, purpose, timestamp, and
  correlation ID.
- `SqlxTenantStoreRegistry::list_active` and the worker's
  `MultiTenantProjectionSchedulerHandler` re-enumerate active holdings every
  pass; suspended tenants are not retained in a process-local allow-list.
- `RoutedTenantProjectionExecutor` resolves server-side write and read routes
  for every claim before invoking an explicitly injected routed projector.
- `WorkerRuntime` exposes a composition helper that builds the scheduler from
  the control pools, registry, authorization context, physical router, and
  explicit executor dependencies.

## Remaining findings

The following remain open and prevent G6/G10/G14 acceptance:

- Universal scheduler/delivery effective-access intersection still needs
  explicit entitlement, billing, module activation, approval-limit, and SoD
  checks where applicable; the scheduler worker permission is not a substitute
  for the full command policy.
- The shipped worker binary still registers only its default handlers. A real
  production projector implementation and executable composition for the
  multi-tenant scheduler are not yet wired into `apps/worker/src/main.rs`.
- Marketing provider callback ingress is still absent: raw-body provider
  signature verification, provider-account binding, durable callback inbox,
  replay/out-of-window policy, and worker-owned delivery transition remain.
- Marketing delivery state transitions still need versioned outbox events and
  a production read projector/rebuild path.
- Scheduler execution still needs a stable job/event/version idempotency
  receipt that covers a crash after target side effects and before completion.
- Registry state is checked before the scheduler operation rather than
  serialized with the registry lifecycle mutation; SQLx integration tests for
  missing, inactive, mismatched, suspended-during-operation, and cross-tenant
  cases remain required.

## Verification

The latest focused checks passed:

- `cargo fmt --all --check`
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings`
- `cargo test -p erpx-infrastructure --lib`
- `cargo clippy -p erpx-worker --all-targets -- -D warnings`
- `cargo test -p erpx-worker --lib` — 36 passed, 1 documented disposable-
  PostgreSQL test ignored

The multi-tenant handler has a pure test proving that active-binding
enumeration issues one claim pass per binding. No live database was mutated by
this follow-up.

## Handoff

Next integration order: authenticated Marketing callback contract and raw-body
ingress; then durable delivery transition events/read projection; then SQLx
scheduler authorization/race tests and final worker executable projector
composition. This follow-up does not approve any completion gate or Folio
write retirement.
