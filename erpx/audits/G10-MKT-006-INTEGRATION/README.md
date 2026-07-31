# G10-MKT-006 integration — Marketing provider callbacks

Status: integrated partial packet. This evidence does not close G10.

## Packet declaration

- **Task ID:** G10-MKT-006-INTEGRATION
- **Objective:** Carry the authenticated Marketing callback contract through a
  raw-body HTTP boundary into the tenant write database with deterministic
  inbox idempotency, provider-reference binding, ordered stale-event handling,
  correlation retention, and an atomic delivery snapshot/outbox transition.
- **Dependencies:** G10-MKT-005 delivery aggregate/persistence; pure callback
  contract from G10-MKT-006; tenant write/read routing and migrations.
- **Allowed paths:** Marketing callback domain integration, tenant delivery
  persistence and migrations, server HTTP callback boundary, migration tests,
  and this audit.
- **Forbidden paths:** Folio routes/database, provider credentials, long-lived
  dual writes, and unrelated ERP contexts.
- **Produced interfaces:** `POST /webhooks/marketing`,
  `SqlxMarketingDeliveryStore::accept_provider_callback`, migration 0027, and
  `MarketingProviderCallbackReceipt`.
- **Database effects:** Tenant-write callback inbox with forced holding RLS,
  unique provider-event identity, verified-body digest, verifier key reference,
  correlation ID, order classification, and JSON contract metadata. Accepted
  in-order callbacks update the delivery aggregate and append a versioned
  callback outbox event in the same transaction.
- **Rollback:** Disable the callback route and drain/replay the additive inbox;
  do not remove migration 0027 or delivery rows while provider retries may
  still arrive.

## Delivered behavior

- The HTTP boundary verifies the exact raw body with HMAC-SHA256 before JSON
  parsing, enforces a bounded timestamp window, and requires a configured
  provider account, opaque tenant/holding route, and verifier key reference.
- The control plane resolves the provider account against an active tenant
  registry/holding binding and matching verifier-key reference before the
  callback can route to a tenant database. Suspended accounts fail closed.
- The domain callback is bound to the trusted route and exact delivery ID,
  scope, and stored provider message reference.
- The tenant-write inbox stores only typed callback metadata and the SHA-256
  body digest; secrets, signatures, and raw provider credentials are not
  persisted.
- Exact duplicate events are safe no-ops. Same-provider-event fingerprint
  conflicts fail closed. Older events are retained without regressing the
  delivery aggregate.
- Delivered, failed, and bounced outcomes are applied only from the accepted
  provider state and emit `marketing.delivery_callback_applied.v1` atomically
  with the delivery snapshot.

## Files changed

- `crates/domain/src/marketing_callback.rs`
- `crates/domain/src/marketing_delivery.rs`
- `crates/infrastructure/src/marketing_delivery_persistence.rs`
- `crates/infrastructure/tests/marketing_delivery_persistence.rs`
- `crates/infrastructure/tests/marketing_provider_accounts.rs`
- `crates/migration/migrations_tenant_write/0027_marketing_provider_callback_inbox.sql`
- `crates/migration/migrations/0026_marketing_provider_accounts.sql`
- `crates/migration/src/lib.rs`
- `apps/server/src/main.rs`
- `MASTER_PLAN.md`
- `EXECUTION_LEDGER.md`

## Independent verification

From `erpx/`:

- `cargo fmt --all --check` — PASS.
- `cargo clippy -p erpx-domain --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-domain --lib` — PASS: 135 passed.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-infrastructure --lib` — PASS: 91 passed, 4 ignored.
- `cargo clippy -p erpx-infrastructure --test marketing_delivery_persistence -- -D warnings` — PASS.
- `cargo test -p erpx-infrastructure --test marketing_delivery_persistence -- --list` — PASS; integration test listed.
- `cargo test -p erpx-infrastructure --test marketing_delivery_persistence -- --ignored --nocapture` — PASS: 1 passed against two explicitly named temporary local PostgreSQL databases; both were removed by cleanup.
- `cargo clippy -p erpx-infrastructure --test marketing_provider_accounts -- -D warnings` — PASS.
- `cargo test -p erpx-infrastructure --test marketing_provider_accounts -- --ignored --nocapture` — PASS: 1 passed against two explicitly named temporary local PostgreSQL databases; both were removed by cleanup.
- `cargo test -p erpx-migration` — PASS: 6 passed.
- `cargo clippy -p erpx-server --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-server --bin erpx-server` — PASS: 54 passed.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-worker --lib` — PASS: 36 passed, 1 ignored.

## Open requirements

Provider-specific adapter parsers and credential rotation, live channel calls,
worker callback consumption and reconciliation, full delivery read projection
execution, journeys/CRM/attribution, authenticated Marketing UI, and the G10
gate remain open.
