# G5-002 Catalog Provider Boundary Evidence

## Delivered behavior

- Added a repository-pinned Stripe API version (`2026-04-22.dahlia`) and
  exact production API base URL validation. `StripeProviderConfig` contains
  no API key, webhook secret, credential value, or serialized secret field.
- Added validated typed identifiers for Stripe accounts, products, prices,
  entitlement features, and billing meters. A typed provider snapshot binds
  those identifiers to the normalized catalog revision and rejects missing,
  duplicate, unexpected, or mismatched references.
- Added `StripeCatalogPublishRequest` and
  `StripeCatalogReconcileRequest`. Publication requires a non-empty bounded
  idempotency key; reconciliation validates the local published revision and
  any expected provider revision reference.
- Added `StripeCatalogProvider`, with every operation receiving the validated
  `StripeProviderConfig`, plus `StripeCatalogProviderBoundary`, which validates
  configuration and DTOs before invoking an adapter and validates its returned
  snapshot before producing the deterministic existing
  `CatalogReconciliationReport`.
- Added `StripeProviderError` and `redact_provider_error`. Only allow-listed
  provider error type/code tokens and a safe request ID survive; raw response
  bodies, URLs, headers, payment identifiers, tokens, and secret-like values
  are not retained or displayed.
- Added no Stripe client, network fallback, fake production adapter, raw-body
  changes, database changes, migrations, or provider-side mutation behavior.

## Files

- `crates/infrastructure/src/commercial.rs`
- `crates/infrastructure/src/lib.rs` — public re-export boundary for the new
  commercial provider types, required because the commercial module is private
  at the crate root.
- `audits/G5-002-CATALOG-PROVIDER-BOUNDARY/evidence/README.md`

The domain catalog and deterministic reconciliation implementation consumed by
this packet remain in `crates/domain/src/commercial.rs`; no domain change was
required for the provider boundary.

## Exact verification

| Command | Result |
|---|---|
| `cargo fmt --all --check` | PASS |
| `cargo test -p erpx-domain commercial::tests` | PASS — 9 passed, 0 failed |
| `cargo test -p erpx-infrastructure --lib commercial::tests` | PASS — 11 passed, 3 expected disposable tests ignored |
| `cargo test -p erpx-infrastructure --tests` | PASS — infrastructure unit tests 22 passed/3 ignored; integration tests compile and all non-disposable tests pass; disposable integration tests ignored |
| `cargo check -p erpx-infrastructure --lib` | PASS |
| `cargo check -p erpx-infrastructure --tests` | PASS |
| `cargo clippy -p erpx-domain -p erpx-infrastructure --all-targets -- -D warnings` | PASS |

Focused tests cover invalid API version/base URL/account configuration, typed
identifier prefixes and snapshot binding, request idempotency and expected
reference validation, deterministic redaction, and rejection before adapter
invocation. No live provider call was made.

## Authority and security boundary

The local published catalog remains ERPX's catalog authority. Stripe remains
the future payment/provider authority once an approved adapter is integrated;
this packet only defines its typed seam. Provider operations cannot be entered
through the boundary without a constructor-validated pinned configuration.
The adapter trait receives that validated configuration explicitly. No
credential source is defined here, and no credential can enter the publish or
reconcile DTOs.

## Residual official Stripe gaps

- Confirm the repository pin against the selected official Stripe OpenAPI
  revision and generate the pinned Rust client.
- Implement the real credential-injecting adapter for Product, Price,
  Entitlement Feature, and Meter publication/fetching, with Stripe idempotency
  headers and raw-response handling outside this packet.
- Map provider objects into `StripeCatalogProviderSnapshot` and schedule
  read-only reconciliation before any approved catalog mutation workflow.
- Define remote catalog metadata persistence and grandfathered migration
  approval/audit integration; no schema or automatic mismatch repair was added.

## Integration order

1. Independently review this type/re-export boundary and confirm the official
   API/OpenAPI pin.
2. Generate the pinned official client in the future adapter workstream and
   inject credentials without changing these DTOs or redaction guarantees.
3. Run provider publication through `StripeCatalogProviderBoundary`, then feed
   its snapshot into the existing deterministic catalog reconciliation seam.
4. Persist/audit reconciliation results and require explicit approval for
   grandfathered price migrations.

## Rollback

Revert the provider types, boundary, re-exports, tests, and this evidence
record. Existing catalog revisions, semantic inbox processing, raw-body
webhook handling, and deterministic local/provider comparison remain readable;
no database or external provider state was changed.
