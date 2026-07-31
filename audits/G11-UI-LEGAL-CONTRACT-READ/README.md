# G11 authenticated Legal contract metadata read

## Scope

The Dioxus Legal route now exposes the existing authenticated
LegalService::ListLegalContracts projection for an explicit tenant, holding,
company, and optional branch UUID scope. The adapter is read-only and keeps
object bytes, object references, content hashes, party identities, and
mutation controls outside the client contract.

## Authoritative path

- The console sends an authenticated/audited query context to the existing
  Legal service.
- The server performs explicit company/branch containment, legal.read
  authorization, customer-scope containment, tenant-read routing, bounded
  cursor pagination, SQLx projection access, retention projection, and audit
  persistence.
- The client retains only contract UUID/reference/title, scoped
  company/branch, lifecycle status, retention state/timestamp, party count,
  approval presence, source version, projection timestamp, and correlation.
- The route renders explicit scope, loading, empty, denied, offline, error,
  and safe read states. Document/object/access commands remain server-owned.

## Fail-closed boundary

LegalContractScope rejects invalid tenant/holding/company/branch references.
LegalContractReadModel::from_wire rejects oversized pages, missing or
contradictory cursors, scope escapes, unsafe values, unknown lifecycle or
retention states, malformed party identity/role metadata, oversized party
sets, zero source versions, and malformed projection timestamps.

## Evidence

- cargo check -p erpx-console --offline --no-default-features --features web:
  PASS.
- cargo test -p erpx-console --offline --no-default-features --features web:
  79/79 PASS, including Legal scope containment, cursor rejection, object
  redaction, lifecycle/retention allow-lists, and party bounds.
- Existing Legal SQLx persistence, outbox/projector, retention, reconciliation,
  and server acceptance suites remain the authoritative domain evidence.

This packet closes only the local Legal contract metadata read adapter. Full
document versions/access history, retention workers, legal workflows, device
journeys, independent database acceptance, migration/cutover, and G0-G14
production evidence remain open. No activation, command-owner switch, Folio
write retirement, or cutover occurred.
