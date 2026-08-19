# G10-MKT-026 — shared CRM display models

## Contract

- **Owner:** AG-UI, independently integrated by the main agent
- **Dependencies:** existing Marketing/CRM route registry and G10-MKT-023
  bounded CRM read transport.
- **Allowed paths:** one target-neutral shared UI module, its export, focused
  tests, this audit, and plan/ledger records.
- **Forbidden paths:** transport/protobuf, server authorization, domain or
  persistence code, provider journeys, Folio, and unrelated UI surfaces.

## Delivered behavior

- Added shared Dioxus CRM display models for accepted-lead handoffs,
  opportunities, and attribution rows.
- Added typed opportunity and attribution status/model labels with Thai and
  English output.
- Added bounded cursor/page metadata and deterministic row sorting.
- Added projection freshness/read-only metadata and explicit Loading, Empty,
  Denied, Ready, Stale, ReadOnly, Conflict, Offline, and Error states.
- Added an allow-listed field boundary that rejects control characters,
  unsupported fields, credential/token/payment markers, and oversized values;
  rejected candidates are counted without being rendered.
- Kept the module presentation-only: it contains no transport, endpoint,
  holding credential, database, or authorization behavior.

## Evidence

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-ui crm -- --nocapture` — 9 PASS, including the existing
  Marketing/CRM route tests.
- `cargo clippy -p erpx-ui --all-targets -- -D warnings` — PASS.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.

## Remaining boundary

Client transport adapters, authenticated holding scope selection, CRM page
rendering and commands, journeys, consent/delivery enforcement, Finance
attribution reconciliation, and the broader G10 gate remain open.

## Rollback

Remove `crates/ui/src/crm.rs` and its shared export. Existing route metadata,
server transport, holding state, and Folio remain unchanged.
