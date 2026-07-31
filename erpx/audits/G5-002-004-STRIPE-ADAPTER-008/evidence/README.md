# G5-002-004-STRIPE-ADAPTER-008 evidence

## Decision

No live Stripe HTTP adapter was added in this packet. The workspace does not
contain a pinned Stripe OpenAPI artifact or generated official client, and no
approved credential-source/runtime contract is available in the allowed paths.
Writing Product, Price, Entitlement Feature, Meter, Checkout, Quote, Customer
Portal, Tax, or Test Clock requests by hand would invent provider behavior, so
the existing injected boundary remains fail-closed and testable without a
network call.

The existing boundary retains the official release pin
`2026-04-22.dahlia`; Stripe documents that release family and its OpenAPI
repository as the source for generated clients:

- https://docs.stripe.com/api/versioning
- https://github.com/stripe/openapi

## Delivered

- Added a regression test proving provider configuration debug output cannot
  contain representative API-key, webhook-secret, or generic secret material.
- Preserved typed provider IDs, pinned-version/HTTPS/account validation,
  provider snapshot validation, deterministic idempotency binding, explicit
  provider-error allowlisting/redaction, and webhook raw-body digest/correlation
  persistence context already present in `commercial.rs`.
- Preserved the injected `StripeCatalogProvider` seam; no fake-success response,
  network call, credential load, HTTP endpoint, or unverified Stripe schema was
  introduced.

## Scope and database effects

- Changed: `erpx/crates/infrastructure/src/commercial.rs` (test only).
- Added: this evidence file.
- Unchanged: `Cargo.toml`; no `stripe.rs` was needed.
- Contract changes: none. The existing provider trait and webhook persistence
  context are unchanged.
- Migrations/database effects: none.
- Server/raw-body wiring: unchanged; the existing persistence context remains
  the handoff boundary for verified raw-body processing.

## Verification

- `rustfmt --edition 2024 --check crates/infrastructure/src/commercial.rs crates/infrastructure/src/lib.rs` — PASS.
- `cargo test -p erpx-domain commercial::tests` — PASS (9 passed).
- `cargo test -p erpx-infrastructure --lib commercial::tests` — PASS (15 passed, 3 ignored).
- `cargo check -p erpx-infrastructure --lib` — PASS.
- `cargo check -p erpx-infrastructure --tests` — PASS.
- `cargo test -p erpx-infrastructure --tests` — PASS (26 passed, 3 ignored; disposable integration tests remain ignored).
- `cargo clippy -p erpx-domain -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo fmt --all --check` — BLOCKED by pre-existing formatting differences in
  forbidden files `erpx/apps/console/src/main.rs`,
  `erpx/crates/application/src/lib.rs`, and
  `erpx/crates/domain/src/support.rs`; no forbidden file was modified. The
  allowed commercial source passes targeted rustfmt above.
- `cargo clippy --workspace --all-targets -- -D warnings` — BLOCKED by existing
  dead-code errors in forbidden `erpx/apps/console/src/main.rs`; the focused
  domain/infrastructure clippy command above passes.
- `cargo test --workspace` — BLOCKED by existing forbidden console compile
  errors (`CustomerAccessAuditSurface` missing and a `CustomerId`/`String`
  mismatch in `erpx/apps/console/src/main.rs`); focused domain and
  infrastructure tests above pass.

No command made a network request or loaded a Stripe credential.

## Open production gaps

1. Pin and vendor/retrieve the approved official Stripe OpenAPI release, then
   generate and review typed models/client coverage for Products, Prices,
   Entitlement Features, Meters, Checkout, Quotes, Customer Portal, Tax, and
   Test Clocks.
2. Inject a production credential source with non-debuggable/zeroizable secret
   handling; bind account, API version, and operation idempotency to the actual
   HTTP request headers.
3. Implement the generated-client transport and typed success/error mapping with
   fake-transport tests, followed by server wiring and disposable/live sandbox
   verification.
4. Confirm Stripe webhook endpoint versioning and raw-body signature handling at
   the forbidden server boundary; this packet does not claim that work.

Official client/API synchronization and live calls were **not achieved**.
G5 is **not complete**.

## Integration order

Official spec pin and generated-client review → injected credential provider →
typed transport and request headers → fake-transport tests → server webhook/
application integration → disposable Stripe sandbox verification → live
credentialed rollout with reconciliation and replay monitoring.

## Rollback

Revert the test-only change and this evidence directory. Existing catalog,
provider-boundary, inbox, and webhook persistence behavior remains readable and
unchanged.
