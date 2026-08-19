# G10-MKT-019 — Marketing-to-CRM handoff and attribution contract

## Objective

Define the ownership boundary where Marketing owns acquisition/qualification
and CRM owns accepted leads, accounts, contacts, opportunities, and revenue
attribution inputs.

## Packet contract

- **Owner:** AG-MKT; main agent independently integrated the domain export.
- **Dependencies:** G0 holding scope/idempotency contracts and existing
  Marketing/Sales domain conventions.
- **Allowed path:** `crates/domain/src/marketing_crm.rs`; main integration also
  exports `marketing_crm` from `crates/domain/src/lib.rs`.
- **Forbidden paths:** migrations, persistence, transport, workers, providers,
  n8n, UI, credentials, and Folio routes.
- **Produced interfaces:** scoped typed references, `AcceptedLeadHandoff`,
  `CrmOpportunity`, `AttributionInputs`, attribution models/results, and
  deterministic idempotency bindings.
- **Database effects:** none.
- **Rollback:** remove the module/export before any persistence adoption.

## Delivered behavior

- Typed Marketing lead/acquisition/qualification references cannot be
  substituted with CRM references and must remain within one company scope.
- CRM opportunity lifecycle is `draft → qualified → won/lost`; invalid skips
  and terminal reuse fail closed.
- First-touch, last-touch, linear, U-shaped, and weighted attribution use
  canonical touch order, exact integer revenue totals, bounded inputs, and
  deterministic remainder allocation.
- Negative revenue/cost, invalid weights, duplicate touches, cross-holding
  references, and conflicting idempotency/reference bindings are rejected.

## Verification

- `cargo fmt --all --check` — PASS after integrating the file.
- `cargo test -p erpx-domain marketing_crm` — PASS; 5 tests.
- `cargo clippy -p erpx-domain --all-targets -- -D warnings` — PASS.

## Known boundary

This is a pure contract slice. SQLx CRM persistence, campaign/journey
delivery, consent enforcement at provider delivery, CRM transport/UI, channel
providers, Finance revenue reconciliation, and attribution read projections
remain open.
