# G7 Operations runtime completion

## Scope

This packet closes the execution-boundary gap around the existing
`OperationsCompletionWorker`. It adds a coordinator-neutral adapter in
`apps/worker/src/operations_reconciliation_runtime.rs` and does not change
worker `lib.rs`/`main.rs` registration, manifests, protobuf, migrations,
server composition, console shell, plans, or ledgers.

The adapter is intentionally compiled as a child of the existing
`operations_completion` module. That path declaration is a testability seam,
not a second handler registration.

## Runtime contract

`OperationsReconciliationRuntime::run_serialized`:

1. rejects empty and oversized serialized capabilities before deserialization;
2. binds the pass's immutable worker UUID to the current worker UUID;
3. requires an explicit holding scope;
4. validates the outer `Reconcile` authorization proof, MFA, validity window,
   and local-only break-glass rules before opening a tenant route;
5. validates nested reassignment/escalation proofs against the same principal,
   holding, and pass timestamp;
6. delegates authoritative writes, outbox repair, projection, and SQLx
   reconciliation to `OperationsCompletionWorker` and its existing stores;
7. returns an error when reconciliation is unhealthy, so a scheduler cannot
   acknowledge a stale or incomplete pass as successful.

No role lookup, local role fallback, customer-directory activation, or
append-only event reversal is performed by this adapter.

## Metrics and rollback evidence

The runtime exposes cumulative redacted counters for authenticated/rejected/
successful/failed passes, projection and lifecycle work, missing outbox rows,
stale aggregates, healthy/unhealthy reconciliation results, and rollback
attempts/outcomes.

Normal completion failures record `rollback_required` and
`attempted = false`. This is deliberate: Operations events and outbox rows are
append-only, so an automatic inverse command would be unsafe. An operator may
submit an independently authenticated pass to `rollback_serialized`; that
path refuses lifecycle mutations and rebuilds only the disposable
`operations_read_model` from the immutable write-side event stream. Its report
records rows/checkpoints cleared, pages read, and events replayed.

## Focused verification

The source module contains four focused tests covering:

- valid worker-bound reconciliation authentication;
- wrong worker and insufficient permission rejection;
- platform-scope and oversized-input rejection before SQLx;
- explicit append-only-safe rollback metrics.

Run them with:

```text
rustfmt --edition 2024 --check apps/worker/src/operations_reconciliation_runtime.rs
cargo test -p erpx-worker --lib operations_completion::reconciliation_runtime
```

Full workspace verification remains coordinator-owned. No live database,
production cutover, or customer-directory activation is implied by this
packet.

## Rollback

Disable the caller that invokes this adapter. Existing immutable Operations
events, aggregate snapshots, source references, and outbox rows remain the
source of truth. If the disposable read model must be recovered, use the
MFA-authorized `rollback_serialized` path and retain its returned metrics as
the rebuild receipt.
