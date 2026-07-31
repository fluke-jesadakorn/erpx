# G11-LEGAL-001 — Legal and document domain foundation

## Contract

- **Owner:** main agent
- **Dependencies:** shared tenant contracts, scope containment, and G11
  Legal/document requirements.
- **Allowed paths:** `crates/domain/src/legal.rs`, its domain export, this
  audit, and plan/ledger records.
- **Forbidden paths:** transport, persistence, workers, UI, migrations,
  MinIO/provider calls, and Folio.

## Delivered behavior

- Added bounded holding/company/branch legal scope with explicit tenant-scope
  conversion and command containment.
- Added contract parties and a closed draft → review → approved → active →
  expired/terminated → archived lifecycle with reviewer separation and
  retention enforcement.
- Added versioned document metadata that stores only opaque object references
  and content hashes; version limits and archive terminality are enforced.
- Added expiring signed-download authorization metadata and immutable access
  history records; raw URLs, tokens, secrets, keys, and payment markers are
  rejected.

## Evidence

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-domain legal::tests -- --nocapture` — PASS; 4 passed,
  0 failed.
- `cargo clippy -p erpx-domain --all-targets -- -D warnings` — PASS.

Persistence, retention jobs, approval authorization, signed MinIO adapters,
transport, read projections, migration fixtures, and the broader G11 gate
remain open.

## Rollback

Remove the `legal` module/export and this audit. Existing domain contexts,
storage adapters, and Folio behavior remain unchanged.
