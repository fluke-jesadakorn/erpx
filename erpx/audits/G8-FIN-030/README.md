# G8-FIN-030 — authenticated Finance close-review read transport

## Contract

- **Owner:** main agent
- **Dependencies:** G8-FIN-018 close/reporting rules, G8-FIN-029 bounded
  close-read adapter, and existing query/auth/routing/audit boundaries.
- **Allowed paths:** Finance protobuf contract, the existing Finance server
  transport, this audit, and plan/ledger records.
- **Forbidden paths:** new close projection migrations/workers, UI, provider
  adapters, unrelated ERP contexts, Folio, and command-owner changes.

## Delivered behavior

- Added typed `GetFinanceCloseReview` protobuf request, bounded review/report/
  metadata response messages, and Finance RPC registration.
- Requires an authenticated query context, explicit holding/company/branch
  scope, `finance.read` role authorization, tenant customer-scope membership,
  and matching requested holding/company identity.
- Rejects platform, subtree, cross-company, malformed UUID/reference, and
  missing-context requests before tenant routing.
- Routes both physically separated tenant pools, invokes the read-only
  `SqlxFinanceCloseReadAdapter`, and returns typed review lifecycle metadata,
  statement/account/control summaries, and explicit missing/fresh/stale/
  unknown projection metadata. Raw JSONB, facts, command contexts, and
  credentials do not cross the boundary.
- Records `finance.close_review.read` in the customer audit sink with actor,
  customer, purpose, and correlation attribution.
- Maps read adapter failures to stable enriched gRPC status categories.

## Evidence

- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-proto` — PASS.
- `cargo check -p erpx-server` — PASS.
- `cargo test -p erpx-server finance_close_read_scope -- --nocapture` — PASS;
  1 passed, 0 failed.
- `cargo test -p erpx-server -- --nocapture` — PASS; 64 passed, 0 failed
  before the additional scope test, then the focused scope test passed.
- `cargo clippy -p erpx-server --all-targets -- -D warnings` — PASS.

The query transport is independently tested at the pure scope boundary. A
disposable two-pool database integration test, dedicated close-review read
projection/worker, and Finance UI remain required.

## Remaining boundary

The adapter currently reads the close snapshot through a read-only query on the
tenant write database while checking Finance journal facts on the tenant read
database. A dedicated close-review read migration/projector must replace that
temporary composition. Close UI, close workers/reconciliation, AP/AR,
consolidation, Thai tax, statements, migration evidence, and the broader G8
gate remain open. This packet does not change command ownership or retire Folio
writes.

## Rollback

Remove the additive protobuf messages/RPC, server scope/helper/response mapping,
and this audit. G8-FIN-028 write transport and G8-FIN-029 infrastructure DTOs
remain available; no database or Folio state is changed.
