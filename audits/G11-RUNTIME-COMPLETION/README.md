# G11 HR/Payroll worker-runtime completion

## Owned slice

This packet is intentionally confined to:

- `apps/worker/src/hr_completion_runtime.rs`
- `audits/G11-RUNTIME-COMPLETION/README.md`

The runtime composes the existing `HrProjectionRunner`,
`HrProjectionRequest`, `SqlxHrProjectionStore`, and typed
`HrProjectionReconciliation` contracts. It does not duplicate HR persistence,
read-model SQL, migrations, transport, authorization, or worker composition.

## Delivered behavior

- Tenant and holding references are validated through the existing bounded HR
  request contract.
- The handler receives a `TenantDatabaseRouter` from the coordinator. The
  existing runner routes the authoritative write/outbox database and the
  disposable read database independently and closes both routes on every
  result path.
- `project_page` applies one bounded source page and returns its continuation
  cursor; a coordinator-owned durable scheduler must persist that cursor before
  claiming another page.
- `reconcile` compares authoritative outbox aggregate versions with read-side
  checkpoints without mutating either side.
- `rebuild` clears and replays the typed read model, then performs a fresh
  reconciliation.
- `reconcile_and_rebuild` repairs only a non-consistent disposable read model;
  a healthy read model is left untouched. Authoritative source rows and
  append-only events are never deleted or rewritten.
- Worker counters are derived from typed reports, and all database failures are
  mapped to operation-only redacted errors. No database URL, SQL text,
  credential, payload, or tenant data is included in an error string.
- Focused tests cover mode parsing, shared bounds, fail-closed references,
  rebuild health decisions, bounded worker counters, and redacted errors.

## Coordinator hook

The coordinator must, in shared registration-owned files:

1. Add `mod hr_completion_runtime;` and re-export
   `HR_COMPLETION_HANDLER_NAME` and `HrCompletionRuntimeHandler` from the
   worker library.
2. In the worker composition root, create
   `TenantDatabaseProvisioningConfig::from_env()`, construct the existing
   `TenantDatabaseRouter` from the control-plane store, and register
   `HrCompletionRuntimeHandler::from_env(router)` when
   `ERPX_WORKER_HANDLER=hr_payroll_completion`.
3. Persist the `project_page` continuation cursor in the authoritative
   scheduler/job state if incremental mode is enabled. Use
   `reconcile_and_rebuild` for an operator-invoked repair pass when no durable
   cursor is supplied.
4. Run the focused worker target after registration, then run the existing
   ignored two-database HR acceptance test and record the live reconciliation
   and rollback result.

## Blockers and rollback

This packet cannot claim live acceptance until the coordinator registers the
module and supplies two disposable tenant databases/control-plane routing.
The existing ignored HR test and any payroll-provider certification still need
real infrastructure; no mock or schema-only result is sufficient. No external
payroll provider, production tenant, Folio write path, or cutover was changed.

Rollback is additive: remove this module and audit directory. Existing HR
authoritative write tables, outbox history, and typed read-model rebuild path
remain unchanged and replayable.
