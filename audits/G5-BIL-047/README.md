# G5-BIL-047 — fail-closed Stripe catalog price reconciliation

The remote Stripe catalog reconciliation boundary now treats an active Price
as ERPX-managed only when all three binding metadata values are present and
valid: `erpx_catalog_revision`, `erpx_catalog_provider_ref`, and
`erpx_catalog_item`. Unmanaged active prices are ignored, while malformed or
conflicting bindings fail closed. This prevents a third-party Price attached
to an ERPX Product from being adopted as the authoritative ERPX price.

Evidence:

- `cargo test -p erpx-infrastructure commercial -- --nocapture` — PASS; 24
  active commercial tests and 4 ignored.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo fmt --all --check` — PASS.

No Stripe network call, credential, migration, worker, server, or Folio write
was used. Remote catalog worker scheduling, metadata onboarding, webhook
replay controls, billing runtime adoption, and the broader G5 gate remain
open.
