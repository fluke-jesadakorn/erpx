# G10-MKT-023 — CRM read transport

## Contract

- **Owner:** main agent
- **Dependencies:** G10-MKT-021 CRM read projection and G10-MKT-022 bounded
  Marketing projection scheduling.
- **Allowed paths:** the holding CRM projection adapter, the versioned platform
  protobuf contract, the Rust server transport, the focused CRM projection
  test, this audit, and the plan/ledger.
- **Forbidden paths:** CRM write aggregates, Marketing delivery/provider
  adapters, Finance, Operations, Folio, environment/runtime state, and
  production command ownership.

## Delivered behavior

- Added a bounded `CrmService.ListCrmReadRows` Tonic/gRPC-Web RPC for accepted
  handoffs, opportunities, and attribution projections.
- Added a holding-read `SqlxMarketingStore::read_crm_page` boundary with
  holding/company/branch filtering, opaque UUID continuation, deterministic
  ordering, and a maximum page size of 100.
- Attribution reads join the projected opportunity to recover authoritative
  company/branch scope before returning a row; attribution is never queried as
  an unscoped table.
- Server transport requires an authenticated request, explicit holding scope,
  the `crm.read` role, an active customer scope binding, holding read routing,
  and the existing query correlation/audit context.
- Response rows contain projection identity, scope identifiers, source version,
  and bounded JSON payload; no holding credentials or provider secrets cross
  the boundary.

## Evidence

- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-infrastructure` — PASS.
- `cargo check -p erpx-server` — PASS.
- `cargo clippy -p erpx-infrastructure -p erpx-server --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-infrastructure marketing_persistence::tests -- --nocapture` — 5 PASS.
- `cargo test -p erpx-server -- --nocapture` — 63 PASS.
- `cargo test -p erpx-migration -- --nocapture` — 6 PASS.
- Disposable `marketing_crm_read_projection` test with physically separate
  holding write/read databases — 1 PASS; opportunity and attribution scope
  reads, wrong-company denial, replay, forced RLS, and rebuild were exercised.

## Security review

- Platform and composite subtree scopes are rejected by this holding read RPC.
- Company and branch predicates are applied in SQL; the attribution query
  derives its scope from the opportunity projection before returning data.
- PostgreSQL read transactions install the routed holding scope, so forced RLS
  remains the final holding fence.
- The server checks the caller's active role and customer scope binding before
  opening the holding read route.

## Remaining boundary

CRM write commands, richer typed CRM transport, CRM UI, profiles/consent,
campaign journeys, provider-specific delivery, attribution projection to
Finance, and the broader G10 gate remain open. This packet does not claim
Marketing or ERPX completion.

## Rollback

Remove the additive CRM service registration and read handler while retaining
the disposable projection schema; no holding write state or Folio behavior is
changed.
