# G5-BIL-046 — remote Stripe catalog reconciliation

The typed Stripe catalog provider now lists bounded Product and Price pages,
selects the latest coherent ERPX metadata revision, validates account/API
response metadata, reconstructs typed provider prices, and returns a
deterministic reconciliation snapshot. Provider failures remain mapped to
the existing redacted error model. No live Stripe credentials or network
calls were used.

Independent evidence:

- `cargo test -p erpx-infrastructure commercial -- --nocapture` — PASS; 23
  active tests, 4 ignored.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo fmt --all --check` — PASS.
- Main-agent repair review added a mock remote metadata reconciliation test;
  it passed and exercised Product/Price list responses, typed identifiers,
  metadata, and in-sync reconciliation.

The provider remains a boundary slice: catalog publication persistence,
scheduled reconciliation worker integration, metadata onboarding for existing
remote objects, Stripe webhook replay controls, and full Billing runtime
adoption remain open.

