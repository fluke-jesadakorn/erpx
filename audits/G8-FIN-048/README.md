# G8-FIN-048 — Bounded authenticated Finance trial-balance read

## Delivered behavior

`FinanceService.GetFinanceTrialBalance` is a read-only Tonic RPC. It returns
deterministic scalar account rows (`account_id`, code, name, kind, debit,
credit, and net minor units), balanced debit/credit totals, chart/company
identity, currency, read-only state, chart source version, projection
timestamp/freshness, and the request correlation ID.

The request is bounded by a 2 MiB chart snapshot and requires an explicit
tenant, holding reference, holding ID, company ID, chart ID, and query
context. The supplied legacy `ChartOfAccounts` snapshot is validated and
cross-checked account-for-account against the authoritative Finance chart read
projection before the existing `SqlxFinanceStore::trial_balance` calculation
runs. Chart lookup is bounded to 100 pages of 100 projected charts. No chart
or journal payload is returned.

## Authorization and routing

- Transport authentication is applied by the existing gRPC boundary.
- The query context must carry an explicit holding, company, or branch scope;
  platform and subtree scopes are rejected.
- The requested company must be contained by the caller scope.
- The caller must hold `finance.read` in that scope and the customer scope
  registry must contain the requested scope.
- The tenant router is used with `RoutedDatabaseSide::Read` only. The existing
  trial-balance store receives the read pool for both constructor pools and
  performs no write operation.
- A redacted `finance.trial_balance.read` audit event records the actor,
  customer, purpose, and correlation ID.

## Bounds and redaction

The chart JSON is limited to 2 MiB and is decoded into the typed Finance
domain model. The projected chart is limited by the existing 4,096-account
projection bound and the 100-page chart lookup bound. Persistence failures are
mapped to stable generic statuses; no SQL, credentials, secrets, or free-form
error text crosses the transport.

## Verification evidence

- `cargo fmt --all --check` — passed during the focused packet verification;
  the later full-worktree rerun was blocked by unrelated formatting changes in
  `crates/migration/src/lib.rs`. `cargo fmt --package erpx-server -- --check`
  passes for the affected server package.
- `cargo check -p erpx-proto -p erpx-server` — passed during focused packet
  verification; the later rerun was blocked by the unrelated migration
  lifetime error at `crates/migration/src/lib.rs:562`.
- `cargo test -p erpx-server` — passed during focused packet verification,
  81 passed, 0 failed, 0 ignored; the later rerun was blocked by the same
  unrelated migration lifetime error.
- `bash audits/G2-IAM-008/verify_authorization_routes.sh` — passed.
- `git diff --check -- crates/proto/proto/erpx/platform/v1/platform.proto apps/server/src/main.rs` — passed.

No production data was queried or modified by the verification commands.

## Unfinished work and security concerns

This packet does not implement Finance UI, AP/AR, bank/cash, close,
consolidation, Thai tax, migration reconciliation, or broader G8 completion.
The current legacy trial-balance domain contract obtains currency from the
validated chart snapshot; the normalized chart read projection does not carry
currency, so a future contract should make currency authoritative in the read
model. The existing `SqlxFinanceStore::trial_balance` also retains its
bounded 500-journal read behavior; an unbounded or paginated financial report
requires a separate infrastructure packet.

## Integration order and rollback

Integrate the proto contract and server implementation together, then run the
focused checks above and the normal integration wave. Roll back only the
G8-FIN-048 changes in `platform.proto`, `apps/server/src/main.rs`, this audit,
and its master-plan section. No migrations, database writes, credentials, or
shared domain/infrastructure files were changed.
