# G6/G12-UI-008 — Platform Operations route registry

## Delivered behavior

- Added target-neutral typed metadata for the seven required Platform
  Operations pages: tenant list/detail, projections, backups, integrations,
  audit, and settings.
- Added platform audience/auth metadata, stable route IDs, opaque tenant
  segment validation, exact segment-aware matching, query/hash normalization,
  and credential-bearing deep-link rejection.
- Added bilingual Thai/English labels and target-neutral navigation items.

## Evidence

- `cargo test -p erpx-ui --lib` — PASS: 55 passed.
- `cargo clippy -p erpx-ui --all-targets -- -D warnings` — PASS.
- Dioxus console web and desktop checks — PASS.
- Folio lint — PASS with four existing warnings; TypeScript — PASS.

## Limits

This packet is metadata only. It does not claim backend platform operations
RPCs, data adapters, authorization, provisioning actions, or UI completion.
