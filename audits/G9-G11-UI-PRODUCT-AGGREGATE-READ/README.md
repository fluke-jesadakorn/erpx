# G9–G11 SCM and Asset/Maintenance authenticated aggregate reads

## Scope

The Dioxus `/procurement`, `/inventory`, and `/maintenance` routes now consume
the existing typed SCM and Asset/Maintenance aggregate read boundaries. Each
route requires an explicit holding reference, holding route reference, holding
UUID, aggregate type, and aggregate UUID. Procurement and Inventory use the
SCM service; Maintenance uses the Asset/Maintenance service.

## Authoritative path

- The console sends authenticated, audited holding-scoped `QueryContext`
  requests to `ScmService::GetScmAggregate` or
  `AssetMaintenanceService::GetAssetMaintenanceAggregate`.
- Server-side authorization, holding routing, immutable source/read
  persistence, outbox/projector behavior, reconciliation, and missing-row
  semantics remain authoritative.
- The adapter validates aggregate identity, correlation, presence/version
  consistency, and the one-megabyte transport bound, then discards aggregate
  JSON before creating the display model.
- The UI exposes only bounded identity, holding, source-version, and
  correlation metadata. Product commands remain server-owned and audited.

## Evidence

- `cargo test -p erpx-console --offline`: 69/69 PASS, including the
  SCM/Asset aggregate identity, missing-response, payload-redaction, and safe
  error-state boundary test.
- `cargo fmt --all --check`: PASS after the adapter and route split.
- Existing SCM and Asset/Maintenance SQLx persistence, projector,
  reconciliation, server, and worker acceptance suites remain the
  authoritative domain/event/read evidence.

This closes only the local SCM/Asset/Maintenance read adapters. Full G9–G11
workflow, vendor/device, browser/device, live reconciliation, rollback, and
production acceptance remain open. No activation, owner switch, Folio write
retirement, or cutover occurred.
