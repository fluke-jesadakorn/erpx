# G8 Finance runtime/reconciliation adapter

## Scope

This packet owns only:

- `erpx/apps/worker/src/finance_completion_runtime.rs`
- `erpx/audits/G8-RUNTIME-COMPLETION/`

The adapter is registered by the coordinator-owned worker root. The executable
composition rechecks local and directory-managed `finance.write` access before
processing claims, while provider credentials and concrete adapters remain
explicit deployment inputs.

## Delivered behavior

- Reuses the existing `FinanceSubledgerReconciliationRunner` for authoritative
  write/read-model reconciliation.
- Reuses the existing `FinanceCloseProjectionRunner` for a bounded sequence of
  close projection pages during reconciliation and source-fenced close
  read-model rollback/rebuild.
- Reuses the existing holding-routed
  `FinanceSubledgerHoldingProjectionExecutor` for an already claimed subledger
  rebuild job.
- Accepts no database URL, credential, or pool; all holding routing remains in
  `HoldingDatabaseRouter` and the existing runners.
- Requires a short-lived, exact-scope authorization proof with the expected
  permission, correlation, purpose, and scheduler authorization handoff.
- Requires MFA for rollback, rejects expired/not-yet-valid proofs, and rejects
  cross-holding/cross-holding projection claims.
- Bounds reconciliation rows at 1,000, close projection pages at 500 with a
  1,000-page pass budget, proof lifetime at five minutes, and each exposed
  rollback metric at 100,000.
- Emits only counts, status, source watermark, and bounded discrepancy metadata;
  no event payload or credential is exposed.
- Keeps rollback limited to disposable read projections. Authoritative
  holding-write aggregates are never rewound by this adapter.

## Focused tests

The new module contains focused unit tests for:

1. request and page/row bounds;
2. exact proof scope and permission binding;
3. MFA and proof lifetime/expiry enforcement;
4. capped rollback metrics with payload redaction; and
5. rebuild-claim kind, lease, projection, and holding/holding validation.

The current worker root registers this handler under
`finance_bank_provider_import` through the explicit authorizer, credential
resolver, adapter factory, holding router, batch bound, and lease. The default
credential resolver and adapter registry are intentionally unconfigured and
fail closed until customer-approved external integrations are injected. The
focused module tests pass in the current workspace.

## Explicit non-claims

This packet does not claim completion of tax behavior, external finance
providers, live PostgreSQL reconciliation, production worker registration,
customer activation, production cutover, or Folio write retirement.

No manifest, lockfile, protobuf, migration, server, console, worker
composition root, plan, or execution ledger was modified.
