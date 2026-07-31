# G5-002-STRIPE-LIVE-PROVIDER-044 evidence

## Delivered behavior

- Added `StripeCatalogHttpClient`, a typed Product/Price transport seam. The
  production implementation delegates to the existing `StripeHttpClient`;
  tests inject a recorder and never construct a network client or load a
  credential.
- Added `StripeHttpCatalogProvider`, an implementation of the existing
  `StripeCatalogProvider` boundary.
- Publication rebuilds the already validated
  `StripeCatalogPublicationPlan`, rejects a mismatched caller key, checks that
  the injected client has the same pinned account/API version/base URL, and
  executes every step in Product-then-Price order.
- Product creation uses `product_idempotency_key`; the subsequent Price
  request uses `price_idempotency_key` and the typed Product ID returned by the
  first call.
- Typed provider IDs are mapped into
  `StripeCatalogPublicationResponse` and then normalized through
  `StripeCatalogProviderSnapshot::new`, so incomplete, duplicate, unexpected,
  or mismatched mappings fail closed.
- Successful responses must be 2xx and carry the configured pinned API
  version. Provider HTTP failures remain allow-listed/redacted; transport,
  decode, credential, timeout, and local boundary failures are converted to
  bounded typed provider errors without copying secrets or raw response data.
- Reconciliation deliberately fails closed because the current typed
  `StripeHttpClient` does not contain an authoritative remote catalog-list
  contract. No fake snapshot is returned.

## Scope and effects

- Changed: `erpx/crates/infrastructure/src/commercial.rs`.
- Changed: `erpx/crates/infrastructure/src/lib.rs` public re-exports.
- Added: this evidence file.
- Contract changes: additive `StripeCatalogHttpClient` and
  `StripeHttpCatalogProvider`; existing catalog plan, provider boundary, and
  typed Product/Price DTOs remain compatible.
- Migration/database effects: none.
- External effects: none in tests; no live Stripe endpoint or credential was
  used.

## Verification

- `rustfmt --edition 2024 --check erpx/crates/infrastructure/src/commercial.rs erpx/crates/infrastructure/src/lib.rs` — PASS.
- `cargo check -p erpx-infrastructure --lib` — PASS; the library compiled with no packet-local warnings after the final import cleanup.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS after the concurrent Operations and Support packets were repaired and formatted.
- No live network or credential-loading test was run.

## Remaining official-client and reconciliation gaps

- The repository still does not contain an approved pinned Stripe OpenAPI
  artifact or generated official Rust client. This packet therefore does not
  claim official OpenAPI generation or generated-client coverage.
- The existing form-encoded typed HTTP client remains the current transport;
  official generated Product/Price/Entitlement Feature/Meter/Checkout/Quote/
  Customer Portal/Tax/Test Clock coverage, credential-source composition,
  remote catalog reconciliation, and server/worker wiring remain open.
- Provider-side retry/replay behavior and disposable Stripe sandbox evidence
  remain open until the approved official client and runtime credential source
  are integrated.

## Security concerns

- The adapter never accepts an API key or webhook secret. The production
  `StripeHttpClient` retains its zeroizable credential wrapper, while the
  injected test seam exposes only typed requests and responses.
- Account, exact HTTPS base URL, and pinned API version are checked both by the
  boundary and the adapter/client pairing. The adapter refuses an unexpected
  response version.
- Error mapping preserves the existing provider redaction contract and uses
  fixed local details for non-provider failures; secrets and raw provider
  bodies cannot enter adapter errors.

## Exact focused tests

Focused commands and results:

```text
cargo test -p erpx-infrastructure --lib commercial::tests::live_catalog_provider
cargo test -p erpx-infrastructure --lib commercial::tests
```

The first command passed 3/3 tests. The second passed 22 tests with 4 expected
disposable-PostgreSQL tests ignored. The suite covers Product-before-Price
ordering, bound idempotency propagation, response ID mapping, Product/Price
mismatch rejection, and redacted provider errors. No command made a network
request or loaded a Stripe credential.

## Integration order

1. Review this adapter and its fake transport seam.
2. Approve and vendor the official Stripe OpenAPI release and generated client.
3. Bind the production credential source and generated transport to the same
   typed seam.
4. Add remote catalog reads/reconciliation and bounded retry/replay evidence.
5. Wire the adapter through server/worker composition roots, then run sandbox
   and production-readiness checks.

## Rollback

Revert only the infrastructure adapter/re-export changes and this evidence
directory. Existing catalog planning, boundary validation, webhook handling,
and Folio operation remain unchanged.
