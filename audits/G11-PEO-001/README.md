# G11-PEO-001 — HR and payroll domain foundation

## Contract

- **Owner:** main agent
- **Dependencies:** shared tenant contracts, scope containment, and the G11
  HR/payroll requirements.
- **Allowed paths:** `crates/domain/src/hr.rs`, its domain export, this audit,
  and plan/ledger records.
- **Forbidden paths:** transport, persistence, worker, UI, migrations, external
  HR/payroll providers, and Folio.

## Delivered behavior

- Added bounded tenant/company/branch `EmploymentScope` with explicit
  `TenantScope` conversion and containment checks.
- Added versioned employee lifecycle with draft, active, leave, suspended, and
  terminal terminated states.
- Added leave requests with bounded reasons, exact date windows, separated
  submission/approval, and terminal cancellation/taken states.
- Added integer-minor-unit expense claims with bounded currency, ordered
  reimbursement lifecycle, and separated approval.
- Added bounded payroll lines and payroll runs with reconciled gross,
  deductions, and net totals; review, approval, posting, and cancellation are
  closed transitions with distinct preparer/reviewer/poster checks.
- Added explicit expired-role workflow reassignment metadata so expired owners
  can be replaced without allowing self-approval or scope escape.

## Evidence

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-domain hr::tests -- --nocapture` — PASS; 3 passed, 0
  failed.
- `cargo clippy -p erpx-domain --all-targets -- -D warnings` — PASS.

The domain tests cover lifecycle terminality, exact scope containment,
approval separation, payroll reconciliation, and unsafe values. Persistence,
authoritative RBAC/SoD integration, transport, read projections, payroll
calculation/provider integration, and the broader G11 gate remain open.

## Rollback

Remove the `hr` module/export and this audit. No existing context, database,
transport, or Folio behavior changes.
