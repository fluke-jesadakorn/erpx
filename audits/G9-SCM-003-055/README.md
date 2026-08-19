# G9-SCM-003 — bounded SCM SQLx persistence

## Delivered behavior

- Added holding-routed SQLx write/read persistence for Product, Warehouse,
  PurchaseRequest, PurchaseOrder, GoodsReceipt, StockMovement, StockTransfer,
  and StockReservation through `SqlxScmStore`.
- Domain aggregates are validated before serialization.
- Every transaction installs the routed `erpx.data_profile_key` setting and relies
  on forced RLS for holding isolation.
- Write transactions enforce optimistic aggregate versions and request-bound
  idempotency receipts, then atomically write source state, the shared
  `aggregate_snapshots` row, the shared `outbox_events` row, and the SCM
  idempotency receipt.
- Read projection application/replay rejects gaps, accepts exact replays as
  no-ops, records contiguous per-aggregate checkpoints, and supports scoped
  rebuild clearing.

## Files changed

- `erpx/crates/infrastructure/src/scm_persistence.rs`
- `erpx/crates/migration/migrations_holding_write/0020_scm.sql`
- `erpx/crates/migration/migrations_holding_read/0020_scm.sql`
- `erpx/audits/G9-SCM-003-055/README.md`

## Contract and migration changes

- Consumes the already integrated public `procurement_inventory` domain
  contracts and existing `PersistenceError`.
- Adds only the packet-owned SCM source, idempotency, read projection, and
  checkpoint tables in migration 0020.
- Does not modify shared migrations, exports, protobuf, server, worker, UI,
  or domain files.

## Exact tests and results

The packet was intentionally kept unexported, so repository tests do not
collect its module tests until the main agent performs the later integration
export. The file was checked with `cargo fmt --all --check`; focused package
checking and tests are reported by the main agent after reviewing this
handoff. The two SQL migrations are embedded by the existing migration
composition root when the packet is integrated.

## Unfinished boundaries

- Main-agent export from `infrastructure/src/lib.rs`.
- Migration assertion updates, application/transport adapters, worker
  registration, source-event consumers, UI, and end-to-end Procure-to-Pay
  reconciliation remain separate packets.
- Disposable database integration tests remain outside this bounded packet.

## Security concerns

- No credentials, secrets, payment instruments, or holding connection details
  are stored.
- Forced RLS and transaction-local holding routing must remain enabled in every
  provisioned holding write/read database.
- Main-agent integration must keep command authorization on the write side and
  must not use the read projection for authorization decisions.

## Integration order

1. Review this file and run focused formatting/checks.
2. Export `SqlxScmStore` and its DTOs from infrastructure.
3. Add migration assertions and run clean/upgrade migration tests.
4. Register application and worker adapters, then verify Procure-to-Pay and
   Operations/Finance event handoffs.

## Rollback

Before export or migration deployment, remove only the three implementation
files and this evidence file listed above. No existing file was edited.
