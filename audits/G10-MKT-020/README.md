# G10-MKT-020 — Marketing-to-CRM SQLx persistence

## Delivered

- Added `SqlxMarketingCrmStore` for accepted-lead handoffs, CRM opportunities,
  and deterministic attribution results.
- Validates domain state and command metadata before persistence.
- Enforces holding-routed transactions, optimistic opportunity versions,
  immutable handoff/source bindings, unique references, and payload-bound
  idempotency receipts.
- Recomputes attribution from the stored opportunity and writes normalized
  touch/result rows; caller-supplied totals are not trusted.
- Commits source rows, shared aggregate snapshots, outbox events, and
  idempotency receipts atomically.
- Added migration `0036_marketing_crm.sql` with forced RLS on every new table,
  composite handoff/opportunity foreign keys, and immutable-row triggers.

## Verification

- `cargo fmt --all --check` — pass after main integration.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — pass.
- `cargo test -p erpx-migration` — 6 passed with migration 0036 assertions.
- `live_marketing_crm_write_boundary_is_atomic_idempotent_and_scoped` — pass
  against a disposable PostgreSQL holding database. The test covers the
  immutable handoff, opportunity creation, a new-context lifecycle transition,
  deterministic attribution, exact replay receipts, four atomic outbox
  events, and forced-RLS metadata; the database was dropped afterward.

## Integration boundary

The main agent exported the store and receipts from `infrastructure/src/lib.rs`
and registered migration 0036 assertions in `migration/src/lib.rs`. A follow-up
domain fix makes every opportunity lifecycle transition bind its command ID and
deterministic idempotency key to the new command context before persistence.

## Unfinished / security notes

- No CRM read-database migration was added: the shared outbox and snapshots
  are the rebuild source; a read projection belongs to the later UI/read-model
  packet.
- The configured opaque `data_profile_key` is the database route boundary; mapping
  that reference to control-plane holding metadata remains outside this packet.

## Integration order

1. Export `marketing_crm_persistence` from infrastructure and its receipt/store
   types.
2. Add migration 0036 structural assertions in the migration crate.
3. Run focused module tests, then the full Rust and Folio verification wave.
