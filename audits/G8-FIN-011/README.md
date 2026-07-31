# G8-FIN-011 — Banking write boundary and reconciliation persistence

## Scope

This packet implements the tenant-local Finance banking write slice. It does
not close the G8 Finance gate and does not claim a bank-provider adapter,
banking read projection, transport, or statutory reconciliation.

## Delivered behavior

- Added `SqlxFinanceBankingStore` for opaque bank-account references,
  immutable statement imports, imported transactions, and reconciliation
  aggregates.
- Revalidated account/import/reconciliation binding against authoritative
  write-side rows inside scoped transactions.
- Added deterministic request hashing and idempotent typed receipts for
  account registration, statement import, and reconciliation writes.
- Enforced duplicate transaction identity and changed-content conflicts.
- Atomically persists aggregate snapshots, outbox events, and idempotency
  receipts with the authoritative rows.
- Added bounded Finance banking source-event pages with stable aggregate/type/
  version cursors for a later worker projector.
- Added tenant-write migration `0028_finance_banking.sql` with forced RLS on
  all banking tables and no provider credentials or payment instruments.
- Kept provider ingestion outside the repository boundary; callers provide
  validated domain facts.

## Changed files

- `crates/infrastructure/src/finance_banking_persistence.rs`
- `crates/infrastructure/src/lib.rs`
- `crates/migration/migrations_tenant_write/0028_finance_banking.sql`
- `crates/migration/src/lib.rs` (migration composition marker)
- `crates/infrastructure/tests/finance_banking_persistence.rs`

## Independent verification

- `cargo fmt --all --check` — PASS.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-infrastructure --lib` — PASS: 93 passed, 4 documented
  disposable-PostgreSQL tests ignored.
- `cargo test -p erpx-infrastructure --test finance_banking_persistence` —
  PASS: 2 active tests passed, 1 documented live test ignored.
- `cargo test -p erpx-migration` — PASS: 6 passed.
- Real disposable PostgreSQL run of
  `live_bank_write_boundary_is_idempotent_scoped_and_reconcilable` — PASS: 1
  passed using separate write/read databases
  `erpx_bank_20260725_w` and `erpx_bank_20260725_r`; both were explicitly
  dropped afterward and a post-test query returned no rows.

## Security and remaining work

Holding-scoped transactions set `erpx.holding_ref` before reads/writes, and
the migration enables and forces RLS for every banking relation. Only opaque
account/provider references are stored. No credentials, tokens, or payment
instruments are accepted or serialized by this packet.

The read pool is reserved for the next projection packet; source-event paging
does not make the write database a read authority. Banking gRPC commands,
read projections/rebuilds, provider adapters, matching workflows, GL posting,
budgets, approval/SoD enforcement, consolidation, Thai tax, statements, and
Finance migration reconciliation remain open.

## Rollback

The packet is additive. Stop any banking worker/transport integration before
removing the store exports and migration after confirming no rows use the
tables. The disposable verification databases were removed; Folio and tenant
command ownership were not changed.
