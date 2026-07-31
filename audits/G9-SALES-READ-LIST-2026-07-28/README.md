# G9/G11 Sales bounded read list — 2026-07-28

## Result

The Sales context now has a real, bounded company/branch read list over the
separate tenant read projection. This packet closes the Sales list-read
implementation slice; it does not claim the full G9 or G11 gate.

## Authoritative path

- `SqlxSalesStore::list_for_scope` reads only the tenant read pool, keyset
  paginates by projected timestamp, allow-listed aggregate type, and UUID, and
  rejects missing or mismatched checkpoints.
- Every row re-decodes the projected Sales aggregate, validates domain identity
  and company/branch containment, and compares typed scope columns with the
  decoded aggregate. Platform, holding, and subtree payloads fail closed.
- `ListSalesAggregates` requires an authenticated explicit product scope,
  `sales.read`, customer-scope containment, tenant read routing, bounded opaque
  cursors, and an append-only audit event. The response exposes metadata only;
  aggregate payload bytes never cross this list boundary.
- The console renders an authenticated read-only Sales table with explicit
  tenant/holding/company/branch scope, loading, empty, denied/offline/error,
  and continuation states. It retains the existing source-owned Sales
  commands and single-aggregate read surface separately.

The source write, idempotency receipt, outbox envelope, worker projector,
checkpoint, reconciliation, and clear/replay rollback behavior remains the
existing authoritative Sales path. This is not a mock or schema-only slice.

## Verification

- `cargo test -p erpx-console sales_list --no-default-features --features desktop --offline` — **1 passed**.
- `cargo test -p erpx-infrastructure sales_persistence --offline --no-default-features` — **6 passed**; ignored routed SQLx tests remain queued.
- `cargo check -p erpx-proto -p erpx-infrastructure -p erpx-server --offline` — passed.
- `cargo check -p erpx-console --offline` — passed.
- The ignored Sales acceptance now asserts scoped list rows, source version,
  sibling-company isolation, reconciliation, and clear/replay rebuild; it
  remains queued until the disposable write/read URLs are supplied.

## Rollback

The RPC and UI are additive. Removing the list route leaves the authoritative
Sales source/outbox/projector and existing single-aggregate read path intact.
Projection rollback continues to clear only the disposable read rows and
replay the source outbox; no source rows, Folio writes, directory state, or
production ownership were changed.

## Remaining acceptance

Full Sales Lead-to-Cash workflow, live routed two-database evidence, worker
restart/replay, accounting and Operations reconciliation, vendor/browser/device
journeys, external directory certification, and production approval remain
queued. G0–G14 are not claimed complete.
