# G4-002 SLA notification persistence — packet 053

## Delivered behavior

- Added control-plane migration `0017_support_sla_notifications.sql` for a
  deterministic notification inbox with customer-visible/internal audience,
  bounded windows, pending/claimed/delivered/expired states, and lease fields.
- `SupportPersistence::enqueue_sla_notifications` validates generated windows,
  inserts by deterministic notification key, treats identical retries as
  no-ops, and rejects changed payloads as idempotency conflicts.
- `claim_due_sla_notifications` uses a bounded `FOR UPDATE SKIP LOCKED` claim
  and a 15-minute lease; expired claims can be retried while unexpired rows
  cannot be concurrently claimed.
- `mark_sla_notifications_delivered` closes only active, unexpired claims.
- The worker `SupportSlaNotificationSource` is implemented by the real
  `SupportPersistence` repository. Delivery provider/channel dispatch remains
  a separate downstream concern.

## Verification

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-infrastructure --lib` — PASS: 64 passed, 4 disposable
  PostgreSQL tests ignored, including the bounded SLA contract test.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-worker --lib` — PASS: 17 passed, 1 disposable test
  ignored.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-migration` — PASS: 6 passed; control migration 0017 is
  embedded and checked.

## Remaining work

This packet does not claim full support notifications: ticket SLA producer
invocation, provider delivery, customer/internal channel adapters, delivery
receipts, and production runtime composition are still open. Database-backed
claim/retry execution requires an isolated disposable PostgreSQL URL.
