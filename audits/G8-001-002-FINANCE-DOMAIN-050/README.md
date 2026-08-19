# G8-001/G8-002 Finance domain foundation

## Delivered

The pure domain boundary now validates charts of accounts, account hierarchy,
journal lines, balanced debit/credit posting, company/holding/scope binding,
open-period posting, explicit period close, immutable posted journals,
idempotent replay, and deterministic trial-balance totals ordered by account
code. Amounts are integer minor units and arithmetic is overflow checked.

## Independent verification

- `cargo fmt --all` — PASS.
- `cargo check --workspace` — PASS.
- `cargo test -p erpx-domain` — PASS: 87 tests, including duplicate-line,
  unbalanced, locked-period, idempotent-posting, and exact trial-balance tests.

The packet is domain-only. SQLx finance repositories/migrations, posting
transport, AP/AR, banking, budgets, intercompany/consolidation, Thai VAT/WHT,
statutory reports, and migration/reconciliation remain open.

## Security and rollback

Commands require a validated command context and holding scope containment;
posted journals cannot be edited and closed periods reject mutation. No
credentials or payment instruments are present. Rollback is a source-only
revert until finance persistence migrations are introduced.
