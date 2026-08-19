# G8-FIN-028 — authenticated Finance close-review command transport

## Contract

- **Owner:** main agent
- **Dependencies:** G8-FIN-018 close/reporting rules, G8-FIN-019
  authoritative close-review persistence, G8-FIN-021 control-scoped close
  evidence, and the established command/auth/routing boundaries.
- **Allowed paths:** Finance close protobuf contract, the existing Finance
  server transport, this audit, and plan/ledger records.
- **Forbidden paths:** new migrations, worker scheduling, provider adapters,
  UI, Folio, command-owner selection, and unrelated ERP contexts.

## Delivered behavior

- Added typed `SaveFinanceCloseReview` protobuf request, receipt, response,
  and Finance RPC registration.
- Decodes bounded typed accounting-period and close-review payloads; validates
  period identity, holding/company binding, explicit company scope, and the
  embedded status-specific command context.
- Requires an authenticated principal equal to the command actor and the
  authoritative Finance entitlement `finance.write`.
- Rejects platform and subtree mutation scopes, invalid transport references,
  missing reviewers/closers, and context mismatches before persistence.
- Routes the mutation to the holding write database only, uses the existing
  `SqlxFinanceCloseStore`, and supports optional optimistic expected-version
  checks.
- Emits a typed receipt and an immutable `finance.close_review.saved` audit
  record. Persistence failures cross the gRPC boundary through the existing
  redacted Finance status mapper.

## Evidence

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-proto` — PASS; 0 tests, no failures.
- `cargo test -p erpx-server -- --nocapture` — PASS; 64 passed, 0 failed.
- `cargo clippy -p erpx-server --all-targets -- -D warnings` — PASS.

The focused server suite exercises the shared authentication, command-context,
scope, audit, redaction, and transport-boundary contracts. A disposable
database integration test for the close-review RPC and production close read,
worker, and UI composition are still required.

## Remaining boundary

Close-review read/report transport, close workers and reconciliation, AP/AR
completion, SoD and approval enforcement, consolidation, Thai tax, statements,
Finance UI, migration/reconciliation evidence, and the broader G8 gate remain
open. This packet does not change command ownership or retire Folio writes.

## Rollback

Remove the additive protobuf messages/RPC and server method. Existing close
domain rules, SQLx persistence, close-control evidence, and Folio operation are
unchanged.
