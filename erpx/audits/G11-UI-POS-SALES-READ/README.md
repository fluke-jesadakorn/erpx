# G11 authenticated POS sales read

## Scope

The Dioxus POS route now exposes the existing authenticated
PosService::ListPosSales projection for an explicit tenant, holding, company,
and optional branch UUID scope. The adapter is read-only and keeps customer
identity, register references, line/payment payloads, and mutation controls
outside the client contract.

## Authoritative path

- The console sends an authenticated/audited query context to the existing POS
  service.
- The server performs explicit company/branch containment, pos.read
  authorization, customer-scope containment, tenant-read routing, bounded
  cursor pagination, SQLx projection access, and audit persistence.
- The client retains only sale reference, currency, bounded monetary totals,
  lifecycle status, line/payment counts with truncation markers, creation time,
  source version, projection timestamp, and correlation.
- The route renders explicit scope, loading, empty, denied, offline, error,
  and safe read states. POS sale commands remain server-owned.

## Fail-closed boundary

PosSaleScope rejects invalid tenant/holding/company/branch references.
PosSaleReadModel::from_wire rejects oversized pages, missing or contradictory
cursors, scope escapes, invalid currency/status values, unsafe references,
oversized line/payment sets, zero source versions, and malformed timestamps.
Truncation markers remain visible so a bounded response cannot be mistaken for
a complete sale payload.

## Evidence

- cargo check -p erpx-console --offline --no-default-features --features web:
  PASS.
- cargo test -p erpx-console --offline --no-default-features --features web:
  80/80 PASS, including POS scope containment, currency/status allow-lists,
  truncation bounds, cursor rejection, and payload redaction.
- Existing POS SQLx persistence, outbox/projector, reconciliation, and server
  acceptance suites remain the authoritative domain evidence.

This packet closes only the local POS sales read adapter. Full POS workflow,
device/payment execution, accounting reconciliation, independent database
acceptance, migration/cutover, and G0-G14 production evidence remain open.
No activation, command-owner switch, Folio write retirement, or cutover
occurred.
