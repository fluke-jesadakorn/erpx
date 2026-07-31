# G8 Finance authenticated chart-of-accounts read

## Scope

The Dioxus /finance route now exposes the existing authenticated
FinanceService::ListFinanceCharts projection for an explicit tenant,
holding, and company UUID. This is a read-only adapter over the routed tenant
read database; it does not infer company scope, reconstruct a chart from
client data, or introduce a chart mutation path.

## Authoritative path

- The console sends an authenticated/audited QueryContext and an explicit
  company-contained holding scope to the server.
- The existing server performs finance.read authorization, customer scope
  containment, tenant-read routing, bounded keyset pagination, and SQLx
  projection access. Existing Finance source events, outbox, projector,
  reconciliation, and rollback behavior remain authoritative.
- The client retains only chart/account identifiers, safe account labels,
  category, normal balance, status, source version, and projection timestamp.
  Source metadata and chart/account payload material are discarded at the
  display boundary.
- The UI renders bounded chart/account metadata with explicit loading, empty,
  denied, offline, and error states. Chart writes, posting, close, and
  provider actions remain server-owned and audited.

## Fail-closed boundary

FinanceChartScope rejects invalid tenant/holding/company references.
FinanceChartReadModel::from_wire rejects oversized pages, missing or
contradictory cursors, invalid chart/holding/company/account identities,
scope escapes, unsafe/unbounded labels, unknown categories, unknown normal
balances, inconsistent parent identity flags, and unknown account statuses.
The account set is capped at 4096 per chart and the page at 100 rows.

## Evidence

- cargo check -p erpx-console --offline --no-default-features --features web:
  PASS.
- cargo test -p erpx-console --offline --no-default-features --features web:
  77/77 PASS, including scope containment, cursor rejection, safe account
  allow-listing, and source-payload redaction.
- The existing Finance server/SQLx/projection acceptance suites remain the
  domain and persistence evidence for this transport.

This packet closes only the local G8 chart-read console slice. Full G8
consolidation, tax/provider/live reconciliation, AP/AR workflows, broader
Finance UI, independent database acceptance, and G0-G14 production/external
evidence remain open. No activation, command-owner switch, Folio write
retirement, or cutover occurred.
