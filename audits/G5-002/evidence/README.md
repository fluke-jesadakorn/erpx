# G5-002 / G5-003 Stripe boundary and billing persistence evidence

## Delivered packet

- Added immutable catalog revision, grandfathered-price migration, billing
  lifecycle, and entitlement allocation persistence on the control write DB.
- Added a durable Stripe event inbox keyed by Stripe event ID with raw body,
  parsed payload, event type/time, processing status, attempts, retry time, and
  last-error fields. Duplicate inserts are harmless.
- Added raw-body HMAC signature verification using the Stripe `t`/`v1`
  signature format and a five-minute timestamp tolerance. Signature validation
  occurs before JSON parsing or persistence.
- Added the `/webhooks/stripe` HTTP boundary with stable 200/400 responses;
  valid duplicate deliveries return 200 without a second inbox row.
- Added worker-facing inbox leases with `FOR UPDATE SKIP LOCKED`, ownership
  checks, bounded exponential retry, dead-lettering, lease-expiry recovery,
  and authorized replay. Claim/state debug output omits payload and raw body.

## Independent verification

```text
cargo fmt --all --check                                      PASS
cargo clippy --workspace --all-targets -- -D warnings       PASS
cargo test --workspace                                      PASS
cargo test -p erpx-infrastructure --test sqlx_control_plane -- --ignored --nocapture  PASS
```

Disposable server evidence on physically distinct PostgreSQL write/read
databases: migrations 0001 through 0005 applied; health/readiness 200; login
and revocation 200 then 401; valid Stripe delivery 200; duplicate delivery
200; invalid signature 400. Both databases and the server were removed after
the run.

## Remaining integration

The pinned official Stripe client, product/price/feature/meter publication,
Checkout/Quotes/Portal/Tax/Test Clocks, out-of-order semantic reconciliation,
worker billing-state application, and universal entitlement/RBAC command
adoption remain open.
