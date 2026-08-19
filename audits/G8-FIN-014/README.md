# G8-FIN-014 — Finance banking gRPC and scheduler integration

## Delivered behavior

The Finance Tonic service now exposes authenticated, typed banking operations:

- `RegisterBankAccount` for opaque provider/account references.
- `ImportBankStatement` for validated statement facts and transactions.
- `SaveBankReconciliation` with optional optimistic expected version.
- `GetFinanceBankingReadRow` for one projected account/import/reconciliation row.

Command handlers require an authenticated actor, command context, explicit
holding scope, Finance role, customer scope, billing/entitlement/SoD checks from
the authoritative command policy, and holding routing. Typed JSON payloads are
bounded and validated before persistence. Write operations use the existing
atomic SQLx store and audit the actor, customer, purpose, action, and
correlation ID. Read operations use only the holding read projection and record a
customer-scoped audit event. No credentials, payment instruments, holding
database URLs, or raw provider payloads cross the transport.

The worker executable can select `ERPX_HOLDING_PROJECTION_KIND=finance_banking`
under the existing durable multi-holding scheduler. The executor validates the
projection name and payload, consumes bounded write-side source pages, applies
idempotent read checkpoints, and supports clear/replay rebuilds. The default
kind remains Operations; no implicit scheduler claiming was introduced.

## Files changed

- `crates/proto/proto/erpx/platform/v1/platform.proto`
- `apps/server/src/main.rs`
- `apps/worker/src/finance_banking_projection.rs`
- `apps/worker/src/lib.rs`
- `apps/worker/src/main.rs`
- `erpx/audits/G8-FIN-014/README.md`

## Verification

Run from `erpx/`:

```text
cargo fmt --all
cargo check -p erpx-server
cargo check -p erpx-worker
cargo clippy -p erpx-server --all-targets -- -D warnings
cargo clippy -p erpx-worker --all-targets -- -D warnings
cargo test -p erpx-server --bin erpx-server
cargo test -p erpx-worker
```

All commands passed. The server suite passed 56 tests. The worker suite passed
43 active tests with one documented disposable-database test ignored.

## Security and unfinished work

The transport boundary is bounded, scope-aware, role-authorized, and
customer-audited. The read RPC returns only typed projection rows. The scheduler
executor treats malformed claims and wrong projection names as permanent
failures and persistence failures as retryable. Routes are closed by the
shared helper on both success and failure.

Provider-specific network adapters, bank-to-GL matching/posting, full Finance
read list/query transport, scheduler claim production, and the remaining G8
workflows are still open. This packet does not claim provider connectivity or
Finance completion.

## Rollback

No Folio ownership, runtime service, environment file, or database migration
changed. Roll back only the additive proto methods, server handlers, worker
selector/executor, and this audit after coordinating generated-client
compatibility; retain previously applied banking migrations and read data.
