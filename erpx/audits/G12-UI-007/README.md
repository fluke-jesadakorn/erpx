# G12-UI-007 — shared Marketing and CRM route scaffold

Status: integrated partial packet. This evidence does not close G12.

## Packet declaration

- **Task ID:** G12-UI-007
- **Objective:** Add typed, target-neutral Marketing and CRM route metadata
  with bilingual labels and safe deep-link handling.
- **Dependencies:** Existing shared Dioxus route, locale, and client-state
  abstractions; G0 shared contracts.
- **Allowed paths:** `crates/ui/src/routes.rs` and `crates/ui/src/i18n.rs`.
- **Forbidden paths:** Server, worker, domain contracts, infrastructure,
  migrations, Folio routes, environment/runtime files, and live data adapters.
- **Database effects:** None.
- **Rollback:** Revert only the two shared UI source files; no runtime or
  database rollback is required.

## Delivered behavior

- Added 23 exact allow-listed RouteId entries for all required `/marketing/*`
  and `/crm/*` pages.
- Added segment-aware `RouteRegistry::marketing_crm()` resolution, static
  English/Thai labels, and safe navigation metadata.
- Credential-bearing deep links are rejected/redacted; existing platform and
  Customer 360 route APIs remain source-compatible.
- No authenticated backend behavior, fabricated live data, or permission
  bypass was added.

## Independent verification

From `erpx/`:

- `cargo fmt --all --check` — PASS.
- `cargo clippy -p erpx-ui --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-ui --lib` — PASS: 50 passed.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- Full workspace wave after integration — PASS; see the execution ledger.

## Open requirements

Authenticated Marketing/CRM data adapters, backend routes, permission checks,
live feature behavior, native secure storage, Android/iOS target evidence,
and the G12 product gate remain open.
