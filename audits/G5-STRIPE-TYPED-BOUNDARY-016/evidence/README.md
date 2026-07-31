# G5-STRIPE-TYPED-BOUNDARY-016

## Delivered behavior

Added a bounded Stripe HTTP provider boundary without server, worker, billing,
database, or webhook wiring.

- `StripeHttpClient` consumes the existing `StripeProviderConfig` and enforces
  the exact HTTPS Stripe endpoint and `PINNED_STRIPE_API_VERSION`.
- `StripeApiKey` validates supported secret-key prefixes, zeroizes on drop, and
  has a redacted `Debug` implementation. It is not part of the provider config
  or any typed request/response envelope.
- Typed request and response envelopes cover product creation/list/retrieval,
  price creation/retrieval, and Checkout Session creation. Forms are encoded
  internally; public APIs do not expose raw JSON values, raw URLs, or header
  maps.
- Every mutating request requires a bounded, non-control idempotency key.
- Request forms, response bodies, and client timeouts are bounded by validated
  limits. Response bodies are consumed in chunks and rejected before exceeding
  the configured maximum.
- Provider failures retain only safe status/category/provider-code/request-ID
  data through the existing redaction boundary. Transport, timeout, body-limit,
  and decode errors do not retain request bodies, URLs, headers, or credentials.
- The HTTP endpoint enum has no webhook variant. Existing raw-body webhook
  verification and `StripeWebhookPersistenceContext` remain separate and
  unchanged.

## Files changed

- `erpx/crates/infrastructure/src/stripe_client.rs`
- `erpx/crates/infrastructure/src/lib.rs`
- `erpx/crates/infrastructure/Cargo.toml`

The new evidence file is this README. No database migrations were added.

## Public API

- `StripeApiKey`, `StripeCredentialError`
- `StripeHttpClient`, `StripeHttpLimits`, timeout/body-size constants
- `StripeRequestEnvelope<T>`, `StripeResponseEnvelope<T>`
- `StripeHttpEndpoint`, `StripeHttpMethod`, `StripeHttpError`, and typed
  request/response models for products, prices, lists, and Checkout Sessions

## Verification

No network call to Stripe was made.

```text
cargo fmt --package erpx-infrastructure -- --check PASS
cargo check -p erpx-infrastructure              PASS
cargo test -p erpx-infrastructure stripe_client::tests -- --nocapture
  7 passed; 0 failed
cargo clippy -p erpx-infrastructure --all-targets -- -D warnings PASS
```

## Remaining provider gaps

- No official generated Stripe OpenAPI client is introduced here.
- No secret-manager implementation, server route, webhook signature handler,
  catalog publication/reconciliation adapter, billing state transition, or
  Stripe reconciliation worker is wired by this packet.
- Product/price catalog mapping, entitlement features/meters, quotes, Portal,
  Tax, Test Clocks, and provider pagination beyond the typed product list must
  be implemented by the billing integration packet.

## Security concerns and integration order

The caller must obtain `StripeApiKey` from the deployment secret source and
must not log or serialize it. Later billing integration should first bind ERPX
operation/revision idempotency keys using the existing commercial abstraction,
then call the typed client. Webhook handling must verify the exact raw request
body before creating `StripeWebhookPersistenceContext`; it must not reuse this
API client for webhook ingestion. Server wiring should map `StripeHttpError`
without adding raw provider bodies or credentials to logs.

## Rollback

Remove the `stripe_client` module/re-export, its `zeroize` dependency entry, and
this evidence directory. Existing commercial abstractions and raw webhook
storage remain independent, so rollback has no database or runtime state effect.
