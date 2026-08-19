# G8-FIN-004-058 — Finance AP/AR and cash-control SQLx persistence

## Delivered behavior

- Added a typed `FinanceSubledgerAggregate` boundary for payable/receivable invoices, vendor/customer payments, allocations, cash receipts, cash disbursements, cash reconciliations, and vendor/customer subledger entries.
- Added holding-routed SQLx write persistence with holding-scoped RLS, bounded holding references, optimistic versions, request-hash idempotency, and atomic aggregate + shared snapshot + outbox + receipt commits.
- Added `save_with_request_key` so a caller can bind a mutation to the current command request instead of reusing the aggregate's original command context key.
- Added an atomic `save_allocation_batch` boundary. It locks and decodes the authoritative payment and invoice rows, repeats domain validation against locked state, applies the domain batch only after all checks pass, and persists all payment, invoice, allocation, snapshot, outbox, and receipt changes in one transaction.
- Added repository-side enforcement that posted/settled/reconciled states retain their explicit `FinanceJournalHandoff`.
- Added bounded source-event paging, idempotent contiguous read projection, replacement-read clearing, and projection checkpoints.

## Files changed

- `crates/infrastructure/src/finance_subledger_persistence.rs`
- `crates/migration/migrations_holding_write/0023_finance_subledgers.sql`
- `crates/migration/migrations_holding_read/0023_finance_subledgers.sql`
- this evidence file

No shared crate exports, existing Finance persistence, application/server/worker/proto/UI, environment, or runtime files were changed.

## Migration effects

Write migration `0023_finance_subledgers.sql` creates the authoritative aggregate and request-idempotency tables, indexes them by holding/company/branch, and enables forced RLS using `erpx.data_profile_key`.

Read migration `0023_finance_subledgers.sql` creates disposable subledger rows and per-aggregate projection checkpoints with the same forced RLS boundary.

## Verification

- `rustfmt --edition 2024 crates/infrastructure/src/finance_subledger_persistence.rs` — PASS.
- `rustfmt --edition 2024 --check crates/infrastructure/src/finance_subledger_persistence.rs` — PASS.
- `cargo fmt --all --check` — blocked by an existing formatting diff in forbidden `crates/infrastructure/src/marketing_persistence.rs`; the new packet file is individually formatted.
- `cargo test -p erpx-migration` — PASS: 6 passed, 0 failed.
- `git diff --check -- erpx/crates/infrastructure/src/finance_subledger_persistence.rs erpx/crates/migration/migrations_holding_write/0023_finance_subledgers.sql erpx/crates/migration/migrations_holding_read/0023_finance_subledgers.sql` — PASS.
- Local pure tests are included in the new persistence file. They become part of the infrastructure test target when the main agent exports the module.

The infrastructure crate was not edited to export this bounded packet, per task restrictions; crate compilation, test collection, and strict Clippy therefore remain an integration-stage check for the main agent. Migration crate tests pass, but they do not yet assert the new version-23 tables.

## Unfinished work

- Export and wire the module from infrastructure `lib.rs`.
- Add migration-version assertions to the shared migration test, outside this packet's allowed paths.
- Add worker routing/projector integration and transport commands.
- Add database-backed integration tests against clean and upgraded holding databases.
- Connect Finance journal aggregate existence checks once the command/application boundary is integrated.

## Security concerns

- The repository sets `erpx.data_profile_key` on every write/read transaction and all new tables use forced RLS; deployment must keep holding DB roles from bypassing RLS.
- Request keys and holding references reject blank, control-containing, or oversized values; secrets are not stored or emitted.
- The repository validates handoff structure and record binding, but authoritative journal existence/status remains an application integration responsibility.
- Allocation batch callers must supply the expected authoritative payment storage version; stale versions fail before mutation.

## Integration order

1. Export the module and public types from infrastructure.
2. Add migration test assertions for both version-23 holding migrations.
3. Run focused infrastructure tests and strict Clippy.
4. Integrate worker source-event routing/projector handling.
5. Add transport/application commands, Finance journal existence checks, and database integration fixtures.

## Rollback

Do not remove the shared snapshots or outbox records. Disable the new Finance subledger command route, leave Folio/legacy Finance ownership unchanged, and retain the additive tables for inspection. If a database rollback is required before production data exists, revert migration `0023` through the normal migration rollback procedure; after data exists, use a forward migration and preserve the audit/outbox history.
