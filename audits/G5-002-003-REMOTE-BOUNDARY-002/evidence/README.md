# G5-002-003 Remote Boundary 002 Evidence

## Audit findings and delivered hardening

- `StripeCatalogProviderSnapshot::validate` now rejects invalid/oversized
  provider revision references in addition to missing, duplicate, unexpected,
  or mismatched typed Product/Price references.
- Reconciliation now enforces
  `StripeCatalogReconcileRequest.expected_provider_revision_ref` and rejects a
  valid-but-unexpected provider response reference.
- Publish idempotency is now bound to the Stripe account, operation, local
  catalog revision, and caller key. The adapter receives the deterministic
  derived provider key rather than an unscoped caller string. Control
  characters and oversized caller keys are rejected.
- Added `StripeWebhookPersistenceContext`, which hashes the exact raw body and
  binds it to a correlation UUID. Added strict `insert_stripe_webhook`; it
  rejects a context generated from different raw bytes before database access.
- Existing migration 0009 already supplies the control-plane
  `raw_body_sha256 BYTEA` length check and correlation index. No migration
  change was needed. New inserts continue to persist the 32-byte digest and
  correlation ID; legacy optional insertion remains for non-HTTP/internal
  callers until the server is migrated to the strict method.
- Expanded dead-letter/provider error redaction for Stripe key prefixes,
  client secrets, private keys, payment identifiers, and existing secret
  markers. The typed provider error remains allow-listed and deterministic.

## Files changed

- `crates/infrastructure/src/commercial.rs`
- `crates/infrastructure/src/lib.rs` — public re-exports for the strengthened
  commercial boundary.
- `audits/G5-002-003-REMOTE-BOUNDARY-002/evidence/README.md`

No changes were made to domain, migration, server/HTTP wiring, identity, UI,
holding provisioning, protobuf, or Folio paths. `migration 0009` was audited
but not modified.

## Exact verification

| Command | Result |
|---|---|
| `rustfmt --edition 2024 --check crates/infrastructure/src/commercial.rs crates/infrastructure/src/lib.rs` | PASS |
| `cargo fmt --all --check` | PASS |
| `cargo test -p erpx-domain commercial::tests` | PASS — 9 passed |
| `cargo test -p erpx-infrastructure --lib commercial::tests` | PASS — 14 passed, 3 expected disposable tests ignored |
| `cargo test -p erpx-infrastructure --tests` | PASS — 25 unit tests passed, 3 ignored; non-disposable integration tests passed; disposable integration tests ignored |
| `cargo check -p erpx-infrastructure --lib` | PASS |
| `cargo check -p erpx-infrastructure --tests` | PASS |
| `cargo clippy -p erpx-domain -p erpx-infrastructure --all-targets -- -D warnings` | PASS |
| Ignored `stripe_inbox_claims_and_transitions_are_durable_and_owned` with two disposable control DBs | PASS — 1 passed; exact digest and correlation persisted; databases dropped and verified absent |

Focused tests cover pinned configuration, typed response references,
account/operation/revision idempotency binding, unexpected reconciliation
references, deterministic error redaction, exact raw-body digest/correlation
context binding, and inbox error redaction.

## Open integration and credential risks

- No official Stripe OpenAPI/client generation, live provider call, credential
  injection, Product/Price/Entitlement Feature/Meter publication, or remote
  fetch was implemented or claimed.
- The server remains on its existing `insert_stripe_event_with_context` call
  because server paths are forbidden in this packet. Main integration must
  switch authenticated HTTP ingestion to `StripeWebhookPersistenceContext` and
  `insert_stripe_webhook`; the legacy optional method must remain limited to
  explicitly non-HTTP/internal callers.
- Migration 0009 permits null digest/correlation for historical rows. New
  authenticated inserts populate both fields; a later retention/backfill audit
  must explain legacy nulls without fabricating request metadata.
- The derived idempotency key is now available to the adapter contract, but
  actual Stripe `Idempotency-Key` transport and provider-side retry behavior
  remain unimplemented.
- Raw bodies remain in the existing control inbox; retention, encryption,
  operator access, and deletion policy are outside this packet.

## Security boundary

The config constructor enforces the repository API-version and exact HTTPS
endpoint pin and contains no credential field. Typed IDs reject arbitrary
provider strings. Provider error bodies are reduced to an allow-list before
they can enter a typed error or dead-letter text. The persistence context binds
the digest to the exact verified bytes and correlation identifier; workers
receive parsed claims and not raw bodies.

## Integration order

1. Main reviews the trait signature and updates the future official adapter to
   use the bound idempotency key.
2. Server integration constructs the strict raw-body context after signature
   verification and uses `insert_stripe_webhook`.
3. Add the pinned official client and credential source without changing the
   DTO or redaction contract.
4. Re-run disposable persistence, signature, retry/DLQ, and provider
   reconciliation tests, then audit legacy null metadata and raw-body
   retention.

## Rollback

Revert the provider validation/idempotency additions, strict persistence
context/method, redaction marker additions, re-exports, tests, and this
evidence. Existing migration 0009, catalog revisions, semantic inbox logic,
raw-body handler, and Folio ownership remain unchanged.
