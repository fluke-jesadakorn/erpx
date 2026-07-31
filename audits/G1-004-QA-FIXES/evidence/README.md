# G1-004 QA fixes — 2026-07-24

This main-agent integration closes the independently reported boundary
findings without changing Folio or runtime secrets.

## Delivered

- Correlation aliases `x-erpx-correlation-id`, legacy `x-erpx-correlation`,
  and `x-correlation-id` are parsed consistently; conflicting aliases fail
  closed, and HTTP/gRPC/audit correlation remains one UUID.
- HTTP 500 maps to `internal`; 502–504 map to `unavailable`, with a focused
  regression test.
- Identity token authentication now uses the same database/record/auth error
  classification as login, refresh, and recovery routes.
- Customer-scoped role renewal now requires customer scope binding, active
  subscription, module entitlement, billing permission, recursive role chain,
  and active SoD. Platform role renewal uses an explicit identity control-plane
  policy with active role chain and SoD; it cannot accept tenant commercial
  claims.
- Worker failures now emit bounded structured context containing worker/event/
  aggregate/correlation identifiers and a redacted message; the durable error
  text remains bounded and does not contain credentials or provider payloads.
- `GetCustomerProjection` exposes the typed Customer 360 detail read envelopes
  through the audited platform gRPC service, including freshness, read-only,
  supported, and unsupported metadata.

## Independent verification

```text
cargo test -p erpx-server boundary::tests -- --nocapture
PASS: 6 passed, 0 failed

cargo test -p erpx-server errors::tests -- --nocapture
PASS: 5 passed, 0 failed

cargo clippy -p erpx-server --all-targets -- -D warnings
PASS

cargo test -p erpx-infrastructure worker::tests -- --nocapture
PASS: worker context and retry tests

cargo clippy -p erpx-infrastructure --all-targets -- -D warnings
PASS

cargo check -p erpx-server
PASS
```

The full repository verification wave remains required. Worker/readiness
coverage, real WebAuthn ceremony, object-access transport, and remaining
context mutation adoption remain open; this report does not close G1-004.
