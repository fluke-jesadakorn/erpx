# G8-FIN-018 — Finance period close and deterministic reporting contract

## Objective

Provide a pure, company-scoped Finance close-review contract that validates
immutable journal facts, unresolved controls, reviewer/closer separation, and
deterministic statement totals and account drill-down without mutating journal
state or taking ownership of posting.

## Packet contract

- **Owner:** AG-FIN; main agent reviewed and integrated the domain export.
- **Dependencies:** G0 command context and scope contracts, G8 journal/period
  domain rules.
- **Allowed path:** `crates/domain/src/finance_close.rs`; main integration also
  exports the module from `crates/domain/src/lib.rs`.
- **Forbidden paths:** persistence, migrations, server transport, workers, UI,
  provider credentials, and Folio routes.
- **Produced interfaces:** `FinanceCloseReview`, `CloseInput`, immutable close
  facts/lines, `StatementTotals`, and deterministic account drill-down rows.
- **Database effects:** none.
- **Rollback:** remove the module/export before any transport or persistence
  adoption; no data rollback is required.

## Delivered behavior

- Close commands require a non-Platform company scope containing the period’s
  holding/company and reject period/fact scope mismatches.
- Close facts require balanced immutable debit/credit lines, nonnegative
  amounts, unique account lines, valid dates, and checked arithmetic.
- Lifecycle is `open → review → closed`; the opener cannot review or close,
  unresolved controls fail closed, and only the recorded reviewer may revise
  review input.
- Statement totals and account drill-down rows are sorted deterministically
  from the validated input facts.
- The contract does not post, mutate, or replace the authoritative journal.

## Verification

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-domain finance_close` — PASS; 6 tests.
- `cargo clippy -p erpx-domain --all-targets -- -D warnings` — PASS.
- The module export was integrated and the full workspace wave remains the
  required follow-up before any broader Finance gate claim.

## Known boundary

This is a pure contract slice. It has no SQLx persistence, close worker,
transport/UI, period-lock orchestration, statutory Thai reporting,
consolidation, or Finance migration/reconciliation implementation.
