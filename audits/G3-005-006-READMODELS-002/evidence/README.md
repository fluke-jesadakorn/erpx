# G3-005-006-READMODELS-002

## Delivered behavior

- Added rebuildable control-read tables for `customer_access_audit_summaries`
  and `customer_notes`.
- Access-audit summaries expose read count, last access time, and the persisted
  `customer_visible` policy bit. An existing customer with no audit rows gets a
  customer-visible zero summary; an unknown customer remains `Absent`.
- Notes are read from the control read pool in deterministic newest-first
  order, with typed ID, author, created time, and body fields.
- Access-audit freshness uses the summary row's `updated_at`; notes freshness
  uses the newest note `updated_at`, falling back to the customer projection
  timestamp when the secondary model is empty.
- All results retain `CustomerReadView` freshness and read-only metadata.
- The existing legacy Customer 360 list/detail port remains unchanged.

## Files

- `crates/domain/src/customer.rs`
- `crates/application/src/lib.rs`
- `crates/infrastructure/src/repositories.rs`
- `crates/infrastructure/tests/customer360_detail_reads.rs`
- `crates/migration/migrations_read/0002_customer360_detail_reads.sql`
- `crates/migration/src/lib.rs` (related migration schema assertion updated from
  one read migration to two)

## Verification

Commands ran from `erpx/`:

```text
rustfmt --edition 2024 crates/migration/src/lib.rs crates/domain/src/customer.rs crates/application/src/lib.rs crates/infrastructure/src/repositories.rs crates/infrastructure/tests/customer360_detail_reads.rs
PASS

cargo check -p erpx-infrastructure --tests
PASS

cargo test -p erpx-domain --lib
PASS: 42 passed, 0 failed

cargo test -p erpx-application --lib
PASS: 4 passed, 0 failed

cargo test -p erpx-migration --lib
PASS: 6 passed, 0 failed

cargo test -p erpx-infrastructure --lib
PASS: 22 passed, 0 failed, 3 ignored

cargo test -p erpx-infrastructure --tests --no-run
PASS: all infrastructure test targets compiled

cargo test -p erpx-infrastructure --test customer360_detail_reads
PASS: 0 passed, 0 failed, 1 ignored (requires two disposable PostgreSQL databases)

cargo clippy -p erpx-domain -p erpx-application --all-targets -- -D warnings
PASS

cargo clippy -p erpx-infrastructure --all-targets -- -D warnings -A dead_code
PASS

cargo clippy -p erpx-infrastructure --all-targets -- -D warnings
PASS: fresh uncached run
```

The physical PostgreSQL test could not run because
`ERPX_TEST_CONTROL_WRITE_DATABASE_URL` and
`ERPX_TEST_CONTROL_READ_DATABASE_URL` were not configured.

## Security and isolation review

- New repository queries use `self.pools.read` exclusively. They do not query
  write-side global audit tables, support/commercial tables, tenant databases,
  credentials, or scope authority.
- The access-audit read model exposes only aggregate customer-visible audit
  fields; actor, scope, purpose, and correlation details remain in the
  write-side audit record and are not copied into this summary DTO.
- The customer projection ID is still checked against its read-model key before
  any secondary view is returned.
- Notes are typed read-side data and are not authorization decisions; transport
  and customer/platform visibility policy remain outside this packet.
- Existing summary redaction remains intact.

## Unfinished work

- A worker/projector still needs to populate these read tables from authoritative
  write-side events; this packet intentionally does not touch worker or write
  models.
- Transport handlers, customer-visible global-read detail views, and UI remain
  future packets.
- Physical disposable PostgreSQL execution remains pending until test database
  URLs are available.

## Integration order and rollback

1. Apply read migration `0002_customer360_detail_reads` to disposable control
   read databases.
2. Run the read projector/backfill to populate audit summaries and notes.
3. Integrate transport only after authorization and audited global-read
   handling consume the typed envelopes.

Rollback removes only the additive read migration, read DTO/query changes,
related tests, and this evidence. It leaves write-side audit authority,
customer projections, credentials, and legacy reads intact.
