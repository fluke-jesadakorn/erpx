# G8-FIN-017 — Finance budget controls and commitment persistence

## Objective

Add a company-scoped, approval-gated Finance budget aggregate and persist
immutable source commitment facts with optimistic versions, atomic outbox
events, and concurrency-safe idempotent replay.

## Packet contract

- **Owner:** main agent; Finance domain and SQLx write-side integration.
- **Dependencies:** G0 shared command context, G1 holding SQLx transaction/outbox
  primitives, G8 chart/journal scope and holding RLS.
- **Allowed paths:** `crates/domain/src/finance_budget.rs`, the domain module
  export, `crates/infrastructure/src/finance_budget_persistence.rs`, Finance
  persistence helper visibility, holding-write migration 0033, migration
  assertions, and the focused integration test.
- **Forbidden shared paths:** Stripe/provider credentials, read-side billing
  state, Folio routes, global-read authorization, and unrelated ERP modules.
- **Produced interfaces:** `FinanceBudget`, lifecycle/approval/control rules,
  `SqlxFinanceBudgetStore`, and `BudgetCommitmentRequest`.
- **Database effects:** migration 0033 adds holding-scoped budget and immutable
  commitment tables with forced RLS, unique idempotency/source references, and
  a database mutation trigger. No Folio schema or read database is changed.
- **Rollback:** roll back code only before applying migration 0033; after
  deployment, stop new commands and drain/reconcile budget facts before any
  schema rollback. Never delete immutable commitment history as rollback.

## Delivered behavior

- Budget lines validate company scope, account/period identity, currency,
  fiscal year, nonnegative amounts, duplicate lines, and overflow.
- The lifecycle is `draft → submitted → approved → locked → closed`; approval
  requires a different principal and an explicit approval limit, and lock
  requires the recorded approver.
- Control checks fail closed before approval, reject negative actuals or
  commitments, and return exact remaining/allowed boundary decisions.
- Budget saves use SQLx optimistic version checks and write snapshots plus
  outbox events atomically.
- Commitment recording acquires a holding/request advisory transaction lock,
  reloads the authoritative budget, rejects over-control, persists immutable
  source facts and the original control decision, and returns an exact replay
  without double-counting even after the budget is closed.

## Verification

- `cargo fmt --all --check` — PASS after the final rustfmt correction.
- `cargo test -p erpx-domain finance_budget` — PASS; 3 tests.
- `cargo test -p erpx-migration` — PASS; 6 tests and doc tests.
- `cargo check -p erpx-infrastructure` — PASS.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- Disposable `live_budget_persistence_enforces_version_control_and_idempotent_commitments` — PASS; fresh holding PostgreSQL migration, lifecycle saves, approved control, exact replay, over-control rejection, and four atomic outbox events were exercised, then the database was dropped.
- The same disposable test also proves changed-actual replay is a typed
  idempotency conflict and that a fabricated approved insert is rejected by
  the persistence boundary. It also closes the budget and replays the stored
  commitment receipt successfully.
- Upgrade-shaped disposable migration 0032 safety test — PASS; malformed,
  overflow, valid, and partial legacy facts were exercised and forced RLS plus
  the immutability trigger were verified after migration.

## Known boundary

This packet does not close G8. Actuals remain authoritative in posted Finance
journals, while commitment source references are currently supplied by the
caller and require integration with Procurement/Sales/Operations producers.
Finance close, AP/AR completion, consolidation, Thai VAT/WHT/statutory
reporting, statements, budget UI/transport, and full SoD/approval enforcement
across every Finance command remain open.
