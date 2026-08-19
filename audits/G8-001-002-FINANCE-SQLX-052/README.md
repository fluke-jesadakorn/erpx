# G8-001/G8-002 Finance SQLx persistence — packet 052

## Objective

Provide the first holding-routed SQLx persistence boundary for the validated
Finance chart, accounting-period, immutable posted-journal, idempotency, and
trial-balance contracts while preserving the physical CQRS split.

## Delivered behavior

- `SqlxFinanceStore` binds every transaction to the validated `data_profile_key`
  through transaction-local `erpx.data_profile_key` and uses separate write/read
  pools.
- Charts and periods use optimistic versions and validated JSON payloads.
- Journal posting validates the domain aggregate, locks the persisted period,
  rejects non-open periods, inserts immutable posted state atomically, and
  appends the aggregate snapshot and holding outbox event in the same
  transaction while storing a request hash for replay-safe idempotency.
- Idempotent replay returns the original posted journal; a changed request for
  the same key fails closed.
- Trial balance reads only the holding read projection and recalculates totals
  through the validated chart contract.
- Holding write migration `0017_finance.sql` and holding read migration
  `0017_finance.sql` are physically separate and force RLS by holding.

## Files

- `crates/infrastructure/src/finance_persistence.rs`
- `crates/infrastructure/src/lib.rs`
- `crates/migration/migrations_holding_write/0017_finance.sql`
- `crates/migration/migrations_holding_read/0017_finance.sql`
- `crates/migration/src/lib.rs`

## Verification

- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-infrastructure` — PASS.
- `cargo test -p erpx-infrastructure --lib` — PASS: 63 passed, 4 disposable
  PostgreSQL tests ignored.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-migration` — PASS: 6 passed; migration separation and
  RLS assertions include Finance version 0017.

## Remaining integration

This packet does not close G8. AP/AR, cash and bank reconciliation, budgets,
approval/SoD enforcement, company books/intercompany, consolidation,
Thailand VAT/WHT/statutory reports, statements/drill-down, Finance transport,
read-model projection workers, migration reconciliation, and end-to-end
Finance/Operations source events remain open. Database-backed execution still
requires isolated disposable PostgreSQL URLs.

## Security and rollback

RLS is forced on every new table; the application sets the holding scope before
queries. Client-supplied balances are never accepted. Posted rows expose no
mutation operation. If the packet is rejected, remove the new Finance 0017
migrations and the new persistence module/export; no existing data or Folio
runtime state is changed.
