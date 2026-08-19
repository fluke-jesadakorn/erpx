# G8-FIN-016 — bank-to-GL matching persistence

## Objective

Persist deterministic bank-to-GL match proposals and explicit decisions on the
holding write database without mutating posted Finance journals. Replays must
return the original typed receipt and conflicting requests must fail closed.

## Packet contract

- **Owner:** AG-FIN for the domain contract; main agent for SQLx integration and
  independent verification.
- **Dependencies:** G8-001/G8-002 posted-journal persistence, G8-FIN-011
  banking write boundary, G8-FIN-012 read/projector seam, holding RLS and
  transaction primitives.
- **Allowed paths:** `crates/domain/src/finance_banking.rs` for the handed-off
  pure contract; `crates/infrastructure/src/finance_banking_persistence.rs`,
  `crates/infrastructure/src/lib.rs`, the holding-write migration, migration
  assertions, and the banking integration test.
- **Forbidden shared paths:** server transport, worker routing, active holding
  switch state, Folio routes, credentials, provider adapters, and posted
  journal mutation code.
- **Produced interfaces:** `BankMatchProposalWriteReceipt`,
  `BankMatchDecisionWriteReceipt`, `save_match_proposal`, and
  `save_match_decision` on `SqlxFinanceBankingStore`.
- **Database effects:** migration 0030 adds holding-local proposal and decision
  tables, holding-scoped RLS, immutable proposal-to-decision foreign-key
  binding, and bounded scope/idempotency indexes. No read database or Folio
  schema is changed.
- **Rollback:** remove the additive migration and adapter methods only after
  draining matching rows; do not delete holding write facts as part of a code
  rollback.

## Delivered behavior

- The domain contract derives candidate order, resolution, and idempotency from
  the authoritative imported transaction and supplied immutable journal IDs.
- The SQLx adapter reloads the bank account and transaction in the write
  transaction, rebuilds the proposal from those facts, and rejects any
  mismatch.
- Every referenced posted journal is locked for share, deserialized as the
  immutable `PostedJournalEntry`, and checked for holding, company, ID,
  entry-date, persisted currency, and persisted total identity before the
  proposal is stored.
- Decisions are validated against the exact stored proposal and recheck its
  authoritative bank facts before insertion.
- Proposal and decision writes use the existing typed request-hash/idempotency
  table and return replay-marked receipts for exact retries.
- Matching records contain no bank credentials, payment instruments, or
  holding connection material.
- Migration 0031 persists journal currency/total facts at posting time and
  installs a database trigger rejecting posted-journal updates and deletes.
  Legacy rows without the new facts fail closed for matching until explicitly
  backfilled.
- Migration 0032 performs a bounded upgrade backfill only from historical
  journal-payload currency and complete serialized line facts. It never uses
  the mutable current chart to invent a historical fact, preserves existing
  partial facts, leaves malformed/overflow/incomplete rows nullable, and
  restores the exact immutability trigger and forced-RLS state.

## Verification

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS after the
  review-only collapsible-if correction in the handed-off domain contract.
- `cargo test -p erpx-domain finance_banking::tests` — PASS; 9 tests.
- `cargo test -p erpx-infrastructure --lib finance_banking_persistence::tests`
  — PASS; 2 tests.
- `cargo test -p erpx-migration --lib
  holding_write_and_read_migrations_are_separate_and_rls_backed` — PASS.
- Disposable live test
  `live_bank_write_boundary_is_idempotent_scoped_and_reconcilable` — PASS;
  fresh write/read databases exercised account/import, posted-journal
  amount/currency matching, exact replay, database-triggered journal mutation
  rejection, and reconciliation, then were dropped.
- Upgrade-shaped disposable holding migration — PASS; migrations through 0030
  were applied using the actual legacy posted-journal payload shape, valid
  debit/credit totals became `NULL:1000` because historical currency is
  absent, invalid amounts remained NULL, partial facts were preserved/
  completed, and forced RLS plus the immutability trigger were restored. This
  is also captured by the ignored committed test
  `legacy_match_fact_backfill_is_safe_for_credit_malformed_overflow_and_partial_rows`
  in `crates/infrastructure/tests/finance_backfill_upgrade.rs`.
- `cargo test --workspace` — PASS in the integration wave; active tests and
  doc tests passed, with only explicitly ignored disposable-database tests.

## Known boundary

This packet does not close the G8 gate: it does not implement Finance close,
tax, consolidation, reporting, or the remaining bank/provider workflows. The
new persisted journal match facts require a reviewed backfill for any legacy
posted rows before those rows become matchable. The historical
`PostedJournalEntry` serializer does not include chart currency, so the
backfill intentionally leaves currency NULL for those rows and fails closed;
only an independently historical, validated payload currency can populate it.
