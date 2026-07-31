# G8-FIN-011 — Finance banking SQLx persistence

## Delivered behavior

This packet adds `SqlxFinanceBankingStore` for the tenant write boundary. It
persists opaque bank accounts, immutable statement imports and transactions,
and mutable reconciliations. Account registration, imports, and reconciliation
writes bind `erpx.holding_ref`, validate the domain payload against authoritative
stored account/import rows, write the specialized state plus shared aggregate
snapshot and outbox event atomically, and record deterministic request hashes
and typed replay receipts.

The transaction identity is unique within the routed holding. Import
idempotency uses the domain's deterministic account/reference key and rejects
changed content. Reconciliation persistence maps the domain's initial version
zero to a one-based source event version and enforces optimistic version
continuity. `read_source_event_page` is bounded and cursor-based for the later
read projector; the dedicated read tables/rebuild adapter are not included
because this packet is restricted to tenant-write migration paths.

No bank credentials, payment instruments, or provider calls are present.

## Files changed

- `crates/infrastructure/src/finance_banking_persistence.rs`
- `crates/infrastructure/src/lib.rs`
- `crates/migration/migrations_tenant_write/0028_finance_banking.sql`
- `crates/infrastructure/tests/finance_banking_persistence.rs`
- `erpx/audits/G8-FIN-011/README.md`

## Verification

Run from `erpx/`:

```text
cargo fmt --all --check
cargo clippy -p erpx-infrastructure --all-targets -- -D warnings
cargo test -p erpx-infrastructure --lib
cargo test -p erpx-infrastructure --test finance_banking_persistence
```

The live test requires `ERPX_TEST_TENANT_WRITE_DATABASE_URL` and
`ERPX_TEST_TENANT_READ_DATABASE_URL`. It never prints either URL. The
disposable write database must be migrated with tenant-write migrations; the
read database is migrated with the existing tenant-read set for the current
source-event seam.

## Security and unfinished work

RLS is enabled and forced on every new table, every store transaction sets the
holding setting, and all queries include the routed holding. Payloads contain
only opaque provider references. The later integration packet must add a
tenant-read migration/read rows, projector checkpoint/rebuild implementation,
account activation/maintenance commands, provider adapter boundary, matching
to Finance journals, and database cleanup around the ignored test.

## Rollback

Do not reset unrelated work. Roll back this packet by removing the module/export,
the isolated test and audit, and reverting tenant-write migration `0028` only
before it is applied to a production tenant. For an already migrated database,
use a reviewed migration to retire these tables after dependent outbox events
and retention obligations are handled; do not drop them ad hoc.
