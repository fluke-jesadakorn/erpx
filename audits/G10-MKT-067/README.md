# G10-MKT-067 — Marketing delivery secret-manager bridge

## Delivered behavior

The worker now supports `MarketingDeliverySecretManagerResolver` with three
explicit modes: the existing `env:NAME` pilot, the shared production callback
secret-manager boundary, and an explicit unconfigured mode. Account-bound
provider construction accepts an injected credential resolver and accepts
validated `secret://`, `sm://`, `vault://`, and `aws-sm://` references in
addition to the pilot env form.

Manager material is copied only across the narrow conversion into the worker’s
zeroizing `MarketingDeliveryCredential`; invalid material is zeroized on
constructor failure. Resolver failures map to bounded worker configuration
categories and never expose references, endpoint data, tokens, or material.
The existing authoritative provider-account key, holding/holding binding,
route, idempotency, and account-version checks remain in force.

## Evidence

- `cargo fmt --all --check` — PASS.
- Worker resolver focused tests — 3 PASS.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.
- `cargo check -p erpx-worker` — PASS.
- `cargo check -p erpx-server` — PASS.
- `git diff --check` — PASS.

Focused tests prove injected manager resolution, redacted credentials,
unconfigured fail-closed behavior, unsupported references, and explicit pilot
env routing.

## Scope and remaining work

This packet composes the shared secret-manager contract into Marketing delivery;
it does not claim vendor-native provider SDKs, delivery-channel onboarding,
provider reconciliation, durable delivery DLQ scheduling, or live external
provider evidence complete. The broader Marketing/ERP, migration, and Folio
write-retirement gates remain open.
