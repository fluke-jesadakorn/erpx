# G5-002 Official Adapter 003 Evidence

## Decision

No production Stripe HTTP adapter was added. The workspace has no official
Stripe OpenAPI artifact or generated client, no approved credential-source
implementation in the allowed infrastructure slice, and no safe basis for
manually inventing Product/Price/Entitlement Feature/Meter endpoint payloads
or response semantics. `reqwest` is already a dependency, but using it with
handwritten speculative JSON would create an unverified provider contract.

The existing typed `StripeCatalogProvider` and guarded boundary remain the
production seam. No fake-success provider, fake HTTP transport, live call,
credential value, HTTP endpoint, or API behavior was added.

## Audited existing guarantees

- The repository-pinned API version and exact HTTPS base URL are validated by
  `StripeProviderConfig`.
- Typed Product/Price/Entitlement Feature/Meter identifiers and provider
  snapshot validation are present.
- Publish idempotency is deterministically bound to account, operation,
  revision, and caller key. The future adapter receives the derived provider
  key through the existing trait contract, but no HTTP header is emitted yet.
- Provider response reference checks and deterministic mismatch reporting are
  present.
- `StripeProviderError` and inbox error paths allow-list safe diagnostics and
  redact credential/payment markers. A regression test now verifies provider
  configuration debug output contains no credential material.
- `StripeWebhookPersistenceContext` binds the exact raw-body SHA-256 digest to
  the correlation UUID; migration 0009 supplies the digest constraint/index.

## Files changed

- `crates/infrastructure/src/commercial.rs` — one focused credential-redaction
  regression test only; no production boundary behavior changed.
- `audits/G5-002-OFFICIAL-ADAPTER-003/evidence/README.md`

`crates/infrastructure/Cargo.toml` was not changed because the existing
`reqwest` dependency is sufficient for a future adapter, but there is no
authoritative client contract to implement safely.

## Exact verification

| Command | Result |
|---|---|
| `rustfmt --edition 2024 --check crates/infrastructure/src/commercial.rs crates/infrastructure/src/lib.rs` | PASS |
| `cargo test -p erpx-domain commercial::tests` | PASS — 9 passed |
| `cargo test -p erpx-infrastructure --lib commercial::tests` | PASS — 15 passed, 3 expected disposable tests ignored |
| `cargo test -p erpx-infrastructure --tests` | PASS — 26 unit tests passed, 3 ignored; non-disposable integration tests passed; disposable integration tests ignored |
| `cargo check -p erpx-infrastructure --lib` | PASS |
| `cargo check -p erpx-infrastructure --tests` | PASS |
| `cargo clippy -p erpx-domain -p erpx-infrastructure --all-targets -- -D warnings` | PASS |
| `cargo fmt --all --check` | BLOCKED by unrelated formatting in forbidden `apps/server/src/main.rs:23`; commercial files pass targeted rustfmt and were not changed to repair server formatting |

No command made a network request. No Stripe credential was loaded or logged.

## Precise adapter gap

1. Obtain and pin the official Stripe OpenAPI revision compatible with the
   repository API version `2026-04-22.dahlia`.
2. Generate and review the official Rust client, including Product, Price,
   Entitlement Feature, Meter, pagination, error, and API-version types.
3. Define an injected secret-key source with a non-debuggable/zeroizable
   credential wrapper and account binding. Do not read credentials from this
   packet or from environment files.
4. Implement the real HTTP transport using the generated client, sending the
   pinned API version, account context, Authorization header, and the existing
   derived `Idempotency-Key` only where the official operation supports it.
5. Map generated responses into `StripeCatalogProviderSnapshot` and map all
   provider failures through the existing redaction contract.
6. Add fake-transport tests only after the generated request/response contract
   exists; test headers, body mapping, pagination, retries, status/error
   mapping, and no secret leakage without network access.

## Open integration and credential risks

- There is no live Stripe client or official API synchronization.
- Credential acquisition, rotation, account authorization, and secure memory
  handling remain unimplemented.
- Actual `Idempotency-Key` header emission and provider retry semantics remain
  unverified.
- Server webhook wiring and raw-body signature handling were intentionally not
  changed.

## Integration order

1. Main approves the official API/OpenAPI pin and generated-client diff.
2. Add the credential source and generated-client adapter in this commercial
   infrastructure boundary.
3. Add deterministic fake-transport tests, then run provider contract tests
   with no live network.
4. Wire the adapter through the worker/server composition roots only after
   credential, retry, redaction, and reconciliation review.

## Rollback

Revert the focused regression test and this evidence record. The existing
typed provider boundary, catalog reconciliation, semantic inbox, webhook
digest/correlation persistence, and Folio ownership remain unchanged.
