# G10-MKT-005-060 — Marketing delivery persistence

## Delivered behavior

- Added tenant-write migration `0026_marketing_delivery.sql` for dispatches
  and deterministic idempotency reservations.
- Enforced holding scope with PostgreSQL forced row-level security.
- Added `SqlxMarketingDeliveryStore` with atomic enqueue, aggregate snapshot,
  and queued outbox persistence.
- Added bounded `FOR UPDATE SKIP LOCKED` due-claiming with worker leases.
- Rechecked consent and suppression before attempt reservation.
- Bound provider-result transitions to the current claim UUID, worker, lease,
  attempt, and deterministic delivery key.
- Bound delivery callbacks to the provider message reference recorded at
  acceptance; signature/authentication remains an external provider-ingress
  responsibility.
- Added optimistic version checks, terminal-state persistence, and bounded
  error/state mappings.

The worker integration adds an explicit `MarketingDeliverySource` and
`MarketingDeliveryProvider` boundary. A named handler is registered only when
both are injected; the default worker registry remains fail-closed.

## Files changed

- `crates/infrastructure/src/marketing_delivery_persistence.rs`
- `crates/infrastructure/src/lib.rs`
- `crates/migration/migrations_tenant_write/0026_marketing_delivery.sql`
- `crates/migration/migrations_tenant_read/0026_marketing_delivery_projection.sql`
- `crates/migration/src/lib.rs`
- `apps/worker/src/marketing_delivery.rs`
- `apps/worker/src/lib.rs`

## Verification

From `erpx/`:

- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-infrastructure --lib` — PASS: 90 passed, 4 ignored.
- `cargo clippy -p erpx-domain --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-domain --lib` — PASS: 122 passed.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-worker --lib` — PASS: 35 passed, 1 ignored.
- `cargo test -p erpx-migration` — PASS: 6 passed.

The ignored infrastructure tests are the repository's documented disposable
PostgreSQL cases; no external tenant database was modified by this packet.

## Security and open work

Holding scope is applied through the tenant transaction and RLS policy;
provider credentials and raw provider payloads are not stored. Provider
submission, authenticated delivery callbacks, durable worker composition,
channel integrations, journeys, CRM, attribution, and full G10 reconciliation
remain open. The persistence boundary does not claim those requirements.

## Rollback

Use the normal forward migration/retention procedure for any applied tenant
database. Do not drop dispatch or idempotency tables while queued or in-flight
work exists. Folio data and runtime state were not changed.
