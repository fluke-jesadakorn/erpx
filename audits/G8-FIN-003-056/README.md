# G8-FIN-003 — Finance AP/AR and cash-control domain contract slice

## Delivered behavior

- Finance-owned vendor and customer party references plus vendor/customer
  subledger entries with integer minor-unit balances and checked application.
- Payable and receivable invoice contracts with validated lines, deterministic
  totals, issue/due-date bounds, approval, posting, settlement, void, and
  terminal close transitions.
- Vendor-payment and customer-payment contracts with authorization, settlement,
  reconciliation, closure, optimistic versions, and no-overpayment guards.
- Allocation records and balanced allocation batches with duplicate detection,
  payment/invoice binding, scope containment, aggregate balance checks, and an
  all-or-nothing in-memory application seam for repository transactions.
- Cash receipt and cash disbursement contracts with payment-kind binding,
  submitted/postable/reconciled/closed lifecycle states, integer amounts, and
  explicit journal handoffs.
- Cash reconciliation status with in-progress, exception, reconciled, and
  terminal closed transitions; out-of-balance completion is rejected.
- Every owned record carries `TenantScope`, `CommandContext`, holding/company
  scope, accounting period/date, deterministic reference and idempotency key,
  and an optimistic version.
- Accounting period/company/date checks use the existing Finance
  `AccountingPeriod` contract. `FinanceJournalHandoff` validates against the
  existing Finance `JournalEntry` identity, company, holding, period, and
  accounting date, and carries an Operations `SourceTransactionRef`.
- Finance remains the AP/AR and cash source owner; Operations is represented
  only by immutable handoff references and is not given source ownership.

## Files changed

- `erpx/crates/domain/src/finance_subledgers.rs`
- `erpx/audits/G8-FIN-003-056/README.md`

No shared files were edited. In particular, `erpx/crates/domain/src/lib.rs`
was not changed, so the repository's `erpx-domain` target does not collect this
module's tests until the main integration packet exports it.

## Contract and migration changes

- New pure domain contracts only.
- No protobuf, persistence, migration, repository, worker, server, UI, Folio,
  environment, or runtime changes.
- No database effects.

## Tests and exact results

The packet was copied into an isolated Cargo harness that supplied the existing
`contracts`, `finance`, `operations`, `access`, and `commercial` modules and
enabled the new module without editing the repository export.

- `rustfmt --edition 2024 --check erpx/crates/domain/src/finance_subledgers.rs`
  — PASS.
- `cargo test --manifest-path /tmp/erpx-finance-subledger-harness/Cargo.toml --lib`
  — PASS: 56 tests, 0 failed, 0 ignored; 8 are the new Finance subledger
  tests.
- `cargo clippy --manifest-path /tmp/erpx-finance-subledger-harness/Cargo.toml
  --lib --all-targets -- -D warnings` — PASS.

The isolated tests cover balanced allocations, overpayment rejection, party and
command scope containment, closed/versioned lifecycle boundaries,
deterministic references and idempotency keys, accounting-period bounds, and
journal-handoff binding.

Repository collection was intentionally not claimed because the requested
`domain/src/lib.rs` export is a forbidden shared-file change for this packet.

## Main integration verification

The main agent exported the module and independently collected its tests:

- `cargo test -p erpx-domain --lib` — PASS: 114 passed.
- `cargo clippy -p erpx-domain --all-targets -- -D warnings` — PASS.

The packet remains a domain contract slice; persistence, transport, and
workflow evidence are recorded separately.

## Unfinished work

- Main integration must export `pub mod finance_subledgers;` from the domain
  crate and rerun repository Finance/domain verification.
- SQLx persistence, separate write/read migrations, outbox/projectors,
  transport, authorization composition, AP/AR workflow UI, bank import,
  reconciliation workers, tax, and migration reconciliation remain separate
  packets.
- A repository transaction must preserve the all-or-nothing behavior of
  `AllocationBatch::apply` and enforce unique idempotency plus optimistic
  concurrency at the database boundary.

## Security concerns

- Scope validation fails closed for platform, holding-only, and subtree record
  scopes; branch records require the matching holding/company/branch identity.
- Party references must contain the record scope and match its holding/company.
- Mutation methods require the caller's expected aggregate version; closed
  records have no valid reopening transition.
- Journal handoffs carry no credentials, payment instruments, or secrets and
  must match the existing Finance journal identity and accounting boundaries.
- Persistence and transport must still apply authoritative RBAC, entitlement,
  billing-state, audit, and segregation-of-duties checks.

## Integration order

1. Main agent exports the module without changing the contracts in this packet.
2. Run `cargo fmt --all --check`, `cargo test -p erpx-domain`, and strict
   domain Clippy; independently review the new tests after collection.
3. Add tenant write/read persistence and transactional outbox/idempotency
   enforcement.
4. Add Finance transport/application authorization and journal posting
   handoffs.
5. Add AP/AR, cash, bank reconciliation, Operations event, and migration
   reconciliation workflows.

## Rollback

Remove only `erpx/crates/domain/src/finance_subledgers.rs` and this evidence
directory. No shared source, migration, database, or runtime state was
changed by this packet.
