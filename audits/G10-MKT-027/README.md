# G10-MKT-027 — CRM client/page-rendering slice

## Scope

This packet adds the next read-only CRM client slice to the shared Dioxus
console. It consumes the existing `ListCrmReadRows` protobuf service and the
G10-MKT-026 shared CRM display models. It does not change protobufs, server
authorization, domain contracts, persistence, migrations, command ownership,
or Folio.

## Delivered behavior

- Exposes the existing 23-entry Marketing/CRM route registry from the console
  navigation with English/Thai route labels.
- Maps only already-supported read aggregates:
  - `/crm/leads` → `marketing_crm_accepted_lead_handoff`.
  - `/crm/opportunities` → `marketing_crm_opportunity`.
  - `/marketing/attribution` → `marketing_crm_attribution`.
- Renders typed CRM handoff, opportunity, and attribution rows using the
  shared presentation models. Raw payload JSON is never rendered.
- Accepts only bounded, allow-listed string fields from the existing payload
  bytes. Opportunity status and attribution model must be present and parse
  through the shared typed parsers; malformed projections fail closed.
- Builds the existing protobuf request with actor, session, eventual
  consistency, customer-visible audit context, tenant, holding, aggregate,
  and bounded page metadata.
- Rejects platform/subtree scope before transport. The current console has no
  authorized holding/company/branch scope provider, so the live CRM resource
  remains unavailable rather than issuing an unsafe platform-scoped query.
- Renders explicit loading, empty, denied, stale, read-only, offline, and
  unavailable state copy in English and Thai. Unsupported Marketing/CRM routes
  show the typed unavailable/read-only boundary without fabricated records.
- Keeps all client mutation controls absent; server authorization remains the
  authority for future commands.

## Files

- `apps/console/src/main.rs`

No `crates/ui/src/crm.rs` change was required. No protobuf, server, domain,
infrastructure, worker, Folio, database, or migration path changed.

## Verification evidence

Run from `erpx/`:

- `cargo fmt --all` — PASS.
- `cargo test -p erpx-console -- --nocapture` — PASS; 18 passed.
- `cargo clippy -p erpx-console --all-targets -- -D warnings` — PASS.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.

Focused tests cover route allow-list navigation, bilingual labels, explicit
scope construction and platform-scope rejection, typed response decoding,
payload redaction, and all required state categories.

## Unfinished work and concerns

- The console now collects an explicit holding, company, or branch scope;
  server-side customer containment, role, entitlement, and projection checks
  still decide whether the request can produce rows.
- The existing read response has no projection-freshness timestamp, so the
  client preserves the shared stale state but cannot classify a live response
  as fresh/stale yet; successful rows remain read-only with unknown freshness.
- Only the first bounded page is requested. Cursor continuation, CRM account/
  contact/activity adapters, marketing journeys/consent/delivery, attribution
  reconciliation, and mutation UI remain open requirements.
- The client sends the already-held access token only through the existing
  authenticated gRPC metadata boundary; it adds no persistent token storage or
  fallback credential path.

## Integration order and rollback

Integrate after G10-MKT-026 shared CRM models and the existing server
`ListCrmReadRows` transport. Before enabling live data, verify the selector's
tenant/company/branch role, entitlement, audit, and projection-freshness
behavior against a live authorized read. Roll back by removing
the additive CRM navigation/resource/rendering code in
`apps/console/src/main.rs`; existing Customer 360, Platform Operations,
Support, and Incident surfaces are unaffected.

## Scope-provider completion amendment — 2026-07-28

The console now supplies an explicit holding scope for the existing CRM read
transport. The user must enter an opaque tenant reference, holding route
reference, and UUID holding scope, then may select a company or branch UUID
within that holding; UUIDs are canonicalized locally and
credential-marker/control-character input is rejected before a request is
created. Submitting the form activates the existing authenticated
`ListCrmReadRows` resource, while an unselected scope makes no business-data
request. The page control now forwards the server-provided bounded cursor for
continuation and resets it when the route or scope changes. The server remains
authoritative for customer containment, role, entitlement, audit context, and
projection authorization.

Focused evidence:

- `cargo test -p erpx-console --offline crm_holding_scope -- --nocapture` —
  PASS; explicit holding construction, invalid UUID rejection, and sensitive
  input rejection.
- `cargo test -p erpx-console --offline
  crm_response_preserves_bounded_cursor_continuation -- --nocapture` — PASS.
- `cargo test -p erpx-console --offline --no-fail-fast --quiet` — PASS; 85
  console tests.
- `cargo check -p erpx-console --offline --no-default-features --features
  desktop` — PASS.
- `cargo fmt --all --check` — PASS.

Projection freshness, CRM account/contact/activity adapters, Marketing
journeys/consent/delivery,
attribution-to-Finance reconciliation, live vendor certification, and the
remaining G10/G12/G14 gates remain open. No production activation, command
owner switch, Folio write retirement, or cutover occurred.
