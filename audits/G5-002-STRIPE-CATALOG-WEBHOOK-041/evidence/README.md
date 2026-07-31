# G5-002/G5-003-STRIPE-CATALOG-WEBHOOK-041 evidence

## Delivered behavior

- Added `StripeCatalogPublicationPlan`, a deterministic typed work list for a
  published catalog revision. It binds the validated Stripe account, exact
  HTTPS endpoint, pinned API version, revision, catalog operation, and caller
  idempotency key before any provider adapter call.
- Each publication step carries typed Product creation data, metadata linking
  the ERPX revision/item/pricing unit, separate deterministic Product and Price
  idempotency keys, and a typed Price request that binds a validated Product
  identifier after Product creation.
- Added typed Stripe recurring interval/usage fields and bounded metadata
  encoding to the existing HTTP client. Mutations still require a bounded
  `Idempotency-Key`, emit the pinned `Stripe-Version`, and can only target the
  fixed `https://api.stripe.com` endpoint.
- The catalog provider boundary now builds and validates the publication plan
  before invoking an adapter. Invalid catalog requests, unsupported metered
  shapes, oversized item/display values, and missing idempotency are rejected
  before provider invocation.
- Added `StripeCatalogPublicationResponse` and deterministic conversion into
  the existing typed `StripeCatalogProviderSnapshot` and domain
  `CatalogReconciliationReport`. Provider Product/Price identifiers are typed;
  missing or unexpected mappings fail closed.
- Added `StripeEventId` and `StripeWebhookEvent::from_verified_raw_body`. It
  accepts only a previously signature-verified raw body/context pair, checks
  the exact SHA-256 digest and correlation binding, parses the bounded typed
  event envelope, requires an event timestamp and data object, and produces a
  strict SQLx inbox event without exposing raw payloads in debug output.
- Added `SqlxCommercialStore::insert_verified_stripe_webhook`, preserving the
  exact digest/correlation metadata at the inbox boundary. Existing duplicate
  event-ID insertion and semantic timestamp ordering remain idempotent and
  suppress duplicate/out-of-order state changes.
- Added a durable control-plane `replay_count` and bounded replay policy. A
  dead-letter event can be replayed at most three times by default (and a
  caller may select a smaller positive bound); retry attempts remain bounded,
  lease ownership is enforced, and replay exhaustion fails closed.
- Removed the cross-module `Clone` compatibility implementation from
  `crates/infrastructure/src/lib.rs`; `StripeCatalogPublicationPlanError` is
  intentionally not `Clone` because its existing request error is not cloneable.

## Files changed

- `erpx/crates/infrastructure/src/commercial.rs`
- `erpx/crates/infrastructure/src/stripe_client.rs`
- `erpx/crates/infrastructure/src/lib.rs`
- `erpx/crates/migration/migrations/0014_stripe_inbox_replay_bounds.sql`
- `erpx/audits/G5-002-STRIPE-CATALOG-WEBHOOK-041/evidence/README.md`

The later main-agent integration changed `erpx/apps/server/src/main.rs` to
verify the raw signature first, construct `StripeWebhookEvent` from the exact
verified bytes, and call `insert_verified_stripe_webhook`. No Stripe
credential was loaded and no live provider call was made.

## Contract and migration changes

Public infrastructure contracts added or extended:

- `StripeCatalogPublicationPlan`, `StripeCatalogPublicationStep`,
  `StripeCatalogPublicationResponse`, and
  `StripeCatalogPublicationPlanError`.
- `StripePriceInterval`, `StripePriceUsageType`, and
  `StripePriceRecurring`; Product and Price request forms now support bounded
  Stripe metadata.
- `StripeEventId`, `StripeWebhookEvent`, and
  `StripeWebhookEventParseError`.
- `StripeInboxReplayPolicy`, `MAX_STRIPE_INBOX_REPLAYS`, and
  `StripeInboxState.replay_count`.

Migration `0014_stripe_inbox_replay_bounds.sql` is the only database change. It
adds `erpx_control.stripe_event_inbox.replay_count INTEGER NOT NULL DEFAULT 0`
and a non-negative check constraint. It is additive, control-plane-only, and
backwards-safe for existing rows; the new SQLx persistence methods use the
monotonic counter to enforce the replay ceiling.

## Exact verification

All commands were run from `erpx/` unless noted.

| Command | Result |
|---|---|
| `rustfmt --edition 2024 --check crates/infrastructure/src/commercial.rs crates/infrastructure/src/stripe_client.rs crates/infrastructure/src/lib.rs` | PASS |
| `cargo check -p erpx-infrastructure --lib` | PASS |
| `cargo check -p erpx-infrastructure --tests` | PASS |
| `cargo test -p erpx-infrastructure --lib` | PASS — 47 passed, 4 expected disposable tests ignored |
| `cargo test -p erpx-infrastructure --lib commercial::tests` | PASS — 19 passed, 4 expected disposable tests ignored |
| `cargo test -p erpx-infrastructure --lib stripe_client::tests` | PASS — 7 passed |
| `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` | PASS |
| `git diff --check -- erpx/crates/infrastructure/src/commercial.rs erpx/crates/infrastructure/src/stripe_client.rs erpx/crates/infrastructure/src/lib.rs erpx/crates/migration/migrations/0014_stripe_inbox_replay_bounds.sql` | PASS |
| Disposable `stripe_inbox_claims_and_transitions_are_durable_and_owned` with fresh control write/read databases | PASS — 1 passed; digest/correlation, duplicate insert, lease ownership, retry, dead-letter, replay, and processed transition exercised |
| Disposable `stripe_dead_letter_replay_budget_is_durable_and_bounded` with a fresh control write/read database | PASS — 1 passed; migration applied, replay counter persisted, replay limit rejected |

The disposable databases were dropped after each run. Tests use no live Stripe
endpoint.

## Security boundary

The plan contains account/version/operation identifiers but no API key or
webhook secret. HTTP credentials remain in the zeroizing `StripeApiKey` and
are not serializable. Provider errors remain allow-listed/redacted. Webhook
parsing requires the exact verified raw-body context and stores only its digest
and correlation metadata alongside the existing raw inbox record; debug output
and worker claims omit payload/raw body. Replay is monotonic and bounded.

## Residual gaps

- The official generated Stripe OpenAPI client and credential-source/runtime
  adapter are still a later integration concern. The injected
  `StripeCatalogProvider` remains the safe seam; this packet intentionally did
  not invent unverified Product/Price/Entitlement Feature/Meter response
  behavior or make a network call.
- Metered catalog items are rejected until the provider meter/interval contract
  is explicit; silently defaulting an interval would be unsafe.
- Worker composition, authenticated/audited replay, and raw-body retention,
  encryption, and deletion policy remain open. The HTTP server now constructs
  `StripeWebhookPersistenceContext` only after signature verification and uses
  the strict typed insertion method; the legacy optional-context insertion is
  not used for HTTP ingress.
- Raw-body retention/encryption/deletion policy and authenticated operator
  authorization for replay remain outside this infrastructure-only packet.
- Stripe catalog publication still requires later adapter execution, response
  reference acquisition, entitlement feature/meter synchronization, and
  reconciliation job wiring.

## Integration order

1. The approved official Stripe adapter consumes `StripeCatalogPublicationPlan`
   and the per-step typed HTTP requests, preserving the derived idempotency
   keys and pinned configuration.
2. The worker claims the inbox, applies the existing semantic dispatcher, and
   exposes authenticated/audited replay using the bounded policy.
3. Persist/audit provider catalog responses and feed them through
   `StripeCatalogPublicationResponse::into_provider_snapshot` and the existing
   deterministic reconciliation report before entitlement/billing activation.

## Rollback

Revert the three infrastructure source changes, remove migration 0014, and
remove this evidence directory. Existing catalog rows and pre-existing inbox
rows remain readable by the prior code; no external Stripe state was modified.
