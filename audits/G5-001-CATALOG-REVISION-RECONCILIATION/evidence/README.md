# G5-001 Catalog revision reconciliation evidence

## Delivered behavior

- Published revisions now reject zero revision numbers, blank display names,
  missing/duplicate keys, invalid currencies/amounts, invalid trial/coupon
  fields, mismatched setup/coupon currencies, invalid setup fees, and invalid
  overage fields.
- Cross-field invariants are explicit:
  - exactly one flat base fee;
  - modules use seat/company/branch pricing;
  - setup-fee prices are one-time flat prices;
  - metered prices use seat/company/branch units;
  - trials are recurring-only; and
  - included usage is metered-only.
- `PublishedCommercialCatalogRevision::validate()` protects the persistence
  read boundary against invalid serialized rows.
- `ProviderCatalogRevision` and `ProviderCatalogPrice` provide a normalized,
  provider-neutral comparison input without a Stripe SDK or network call.
- `CatalogReconciliationReport` emits typed mismatch categories for missing or
  unexpected prices, duplicate provider keys, revision/reference gaps,
  invariant violations, missing provider references, and individual price
  field differences. Output is deterministically sorted.
- `SqlxCommercialStore::reconcile_catalog_revision` compares an existing local
  control-plane revision with the supplied provider snapshot. Missing local
  revisions are reported, not fabricated.
- Grandfathered migrations remain explicit, ordered, actor/reason-bound
  records; no automatic price migration was introduced.

## Exact verification

| Command | Result |
|---|---|
| `cargo fmt --all --check` | PASS |
| `rustfmt --edition 2024 --check crates/domain/src/commercial.rs crates/infrastructure/src/commercial.rs crates/infrastructure/src/lib.rs` | PASS |
| `cargo clippy -p erpx-domain -p erpx-infrastructure --all-targets -- -D warnings` | PASS |
| `cargo check -p erpx-infrastructure --lib` | PASS |
| `cargo test -p erpx-domain commercial::tests` | PASS — 9 passed |
| `cargo test -p erpx-infrastructure --lib commercial::tests` | PASS — 6 passed, 3 expected disposable tests ignored |
| Disposable `persisted_catalog_reconciliation_is_deterministic_without_provider_calls` | PASS; temporary write/read databases dropped |

`cargo check -p erpx-infrastructure --tests` was rerun after the re-export
repair and is blocked by an unrelated existing test compile error in
`crates/infrastructure/tests/customer360_detail_reads.rs:290`: a generic
`GlobalReadAuditRecord::new` function item is assigned without type
annotations. The infrastructure library itself and all catalog-focused tests
compile successfully; that forbidden customer/test file was not changed.

The broader `cargo test -p erpx-domain` was also run and reported one
unrelated pre-existing failure in
`customer::enriched_projection_tests::unsupported_read_view_serializes_only_metadata_and_reason`;
all 9 catalog tests pass independently. No customer files were changed by
this packet.

## Authority and security boundary

The provider snapshot is data supplied by an already-authorized adapter; this
packet does not grant provider state authority to clients and does not call
Stripe. Local published revisions remain the control-plane catalog authority
until an explicit later reconciliation policy decides how a mismatch is
handled. Comparison output contains catalog metadata only, not payment
secrets, webhook raw bodies, credentials, or tokens.

## Residual live-provider gaps

- No pinned Stripe client or API-version binding.
- No Product/Price/Entitlement Feature/Meter publication or remote fetch.
- No automatic mismatch repair or customer-approved grandfather migration
  workflow.
- Provider-specific translation into `ProviderCatalogRevision` remains a later
  adapter responsibility.

## Integration order

1. Add the pinned provider adapter that produces `ProviderCatalogRevision`.
2. Run comparison in a read-only reconciliation job and persist/audit reports.
3. Require explicit approval for catalog changes and create grandfathered
   migration records where prices differ.
4. Reconcile entitlements and billing authority only after the catalog report
   is resolved.

## Rollback

Revert the catalog invariant/reconciliation additions and the infrastructure
re-export/method. Existing published revision rows remain readable by the prior
catalog persistence code; no schema or provider state is modified.
