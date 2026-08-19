# G8-FIN-005-058 — Finance AP/AR SQLx persistence and journal read transport

## Delivered behavior

- Integrated holding-routed SQLx persistence for payable/receivable invoices,
  vendor/customer payments, allocation batches, cash receipts,
  disbursements, reconciliations, and subledger entries.
- Enforced optimistic versions, request-bound idempotency, forced-RLS scope,
  atomic aggregate/snapshot/outbox/receipt writes, and explicit journal
  handoff requirements for posted states.
- Added disposable Finance subledger read projection, contiguous checkpoints,
  replay, and holding-scoped rebuild clearing.
- Added authenticated `FinanceService.ListPostedJournals` with explicit
  holding/company/branch query scope, bounded page size, holding routing, and
  read-only DTOs.
- Added authenticated `FinanceService.SaveSubledger` for typed AP/AR and
  cash-control aggregates, with scope binding, Finance role and commercial
  entitlement checks, bounded JSON payloads, optimistic/idempotent command
  persistence, and audited actor attribution.
- Added authenticated `FinanceService.SaveAllocationBatch` with typed
  payment/invoice/allocation decoding, explicit scope matching, required
  optimistic payment version, atomic all-or-nothing repository execution, and
  safe receipt mapping.
- Added holding read migration `0023_finance_subledgers.sql` and migration
  assertions.

## Independent verification

- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-infrastructure` — PASS.
- `cargo test -p erpx-infrastructure --lib` — PASS: 79 passed, 4 documented
  disposable-PostgreSQL tests ignored.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-migration` — PASS: 6 passed.
- `cargo check -p erpx-worker` — PASS.
- `cargo test -p erpx-worker --lib` — PASS: 28 passed, 1 documented
  disposable-PostgreSQL test ignored.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-server --bin erpx-server` — PASS: 51 passed.
- `cargo clippy -p erpx-server --all-targets -- -D warnings` — PASS.
- `cargo check -p erpx-server` — PASS.

## Remaining work

Bank import/reconciliation workflows, budgets,
approval-limit and SoD enforcement, company books/intercompany,
consolidation/eliminations, Thai VAT/WHT/statutory reporting, statements,
workflow reconciliation, and the G8 acceptance gate remain open.

## Rollback

Disable the Finance subledger command and read transport routes while keeping
authoritative rows, snapshots, outbox history, and Folio ownership unchanged.
The read projection can be cleared and rebuilt from the holding write stream.
