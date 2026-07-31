# G8 Finance completion handoff

## Scope

This lane adds Finance-owned completion seams without changing shared
registrations, manifests, protobuf, migrations, server transport, console
shell, or worker registration.

Delivered in the allowed Finance ownership boundary:

- `FinanceAuthorizationRequest` and `FinanceAuthorizationDecision` provide a
  transport-neutral authorization contract for the coordinator. It maps
  operations to `finance.read`/`finance.write`, requires MFA for high-risk
  Finance actions, requires a distinct approver for payment approval/release
  and period close, and permits only local, MFA-backed, non-represented-user
  break-glass evidence.
- `SqlxFinanceStore::trial_balance` no longer silently truncates at 500
  journals. It reads the complete authoritative posted-journal set up to a
  bounded one-million-journal report limit and fails closed when that bound is
  exceeded. It continues to derive totals from immutable posted journal facts.
- `SqlxFinanceSubledgerStore::reconcile_read_model` compares bounded,
  decoded authoritative AP/AR/cash aggregate rows in the tenant-write
  database with the disposable tenant-read rows. It checks aggregate identity,
  versions, holding/company/branch scope, and full typed aggregate equality,
  records source high-watermark stability, and never repairs either side.
- `FinanceSubledgerReconciliationRunner` routes both physical database sides,
  invokes the SQLx reconciliation, closes both routed handles, and redacts
  routing/database failures. A non-green report is explicitly handed back to
  the existing projection/rebuild scheduler for repair.

These changes preserve immutable journal handoffs, period and holding/company
identity, append-only source outbox semantics, forced-RLS routing, and the
existing read-model projector. They do not claim consolidation, Thai tax,
external provider certification, live gRPC/UI adapter completion, or final G8
gate closure by themselves.

## Focused evidence

File-scoped Rustfmt and `git diff --check` were run for the permitted files.
Focused Cargo tests/Clippy could not reach Finance compilation because the
existing worktree references the missing non-Finance module
`erpx/crates/domain/src/operations/completion.rs` from
`crates/domain/src/operations.rs` (`E0583`). That blocker was not changed in
this lane. No live PostgreSQL proof is claimed here; the reconciliation
adapter is implemented against the real routed SQLx pools and requires the
existing tenant migrations for an integration run.

## Coordinator integration hooks

1. Re-export the new domain authorization and reconciliation types from the
   existing Finance-facing shared surfaces as needed; keep the shared
   authorization evaluator responsible for permission, entitlement, customer
   scope, and audit checks, then call `FinanceAuthorizationRequest::evaluate`
   for the Finance-specific high-risk/SoD evidence.
2. Expose the Finance transport command/query adapters through the existing
   server/protobuf/UI owners. The contract returns only bounded scalar
   decision/report data; do not expose aggregate payloads, command contexts,
   SQL details, or evidence material.
3. Add the reconciliation runner to the existing durable Finance subledger
   operations/scheduler registration. Treat `report.is_reconciled()` as the
   green condition; route `issues`, incomplete bounds, or source movement to
   an explicit projection/rebuild job and keep the read surface stale/read-only
   until it is green.
4. Run a fresh two-database PostgreSQL proof after the shared module blocker is
   resolved: seed authoritative subledger rows and outbox events, project a
   read row, verify a green report, then prove missing-row, stale-version,
   scope/payload mismatch, unexpected-row, source-movement, forced-RLS, and
   cleanup behavior. Keep database URLs and failure details redacted.
5. Run the full coordinator-owned verification gates, including workspace
   format/Clippy/tests, ignored database tests, protobuf compatibility,
   Dioxus targets, `bun run lint`, and `bunx tsc --noEmit`. Keep Folio on port
   3004; no command-owner switch, directory activation, write retirement, or
   production cutover is authorized by this packet.

## Rollback

Rollback is file-scoped and reversible: revert only the Finance-owned paths
listed below and remove this evidence directory. No migration, database,
outbox row, credential, registration, or production state is changed by the
rollback itself.

- `crates/domain/src/finance.rs`
- `crates/domain/src/finance_subledgers.rs`
- `crates/infrastructure/src/finance_persistence.rs`
- `crates/infrastructure/src/finance_subledger_persistence.rs`
- `apps/worker/src/finance_subledger_projection.rs`
- `audits/G8-COMPLETION/README.md`
