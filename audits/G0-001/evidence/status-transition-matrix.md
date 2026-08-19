# Status/transition matrix at G0-001

## Folio source workflow

| Context | States observed | Enforcing source |
|---|---|---|
| Expense/waybill approval | submission, department_approval, accounting_review, accounting_approval, executive_approval, payment, settlement, awaiting_disbursement, disbursed, rejected, gl_confirmed | `lib/perm/stages.ts`, `app/actions/waybill.ts`, `app/actions/expense.ts` |
| Purchase workflow | draft, submission, dept_verification, dept_authorization, accounting_verification, accounting_supervision, accounting_authorization, disbursement_authorization, cfo_authorization, ceo_authorization, awaiting_disbursement, disbursed, rejected | `lib/perm/stages.ts`, `app/actions/procurement.ts`, `app/actions/waybill.ts` |
| Sales workflow | so_draft, so_sales_review, so_dept_approval, so_credit_check, so_invoiced, so_paid | `lib/perm/stages.ts`, `lib/operations/dashboard.ts`, sales actions |

Folio actions use row locks and compare the submitted stage with the current database stage before mutation. Exact cross-context transition completeness is a G9/G13 reconciliation requirement.

## Platform prototype workflow

| Context | States observed | Enforcing source |
|---|---|---|
| Customer | prospect, onboarding, active, suspended, churned | `db/migrations/2026-07-23-erpx-control-plane.sql`, `lib/platform/types.ts` |
| Billing | trialing, active, past_due, grace, read_only, canceled | Platform SQL check, `erpx/crates/domain/src/commercial.rs` |
| Support ticket | new, triaged, waiting_for_customer, access_requested, approved, in_progress, resolved, customer_confirmed, closed | Platform SQL check, `erpx/crates/domain/src/support.rs` |
| Support mode | diagnostic_read ≤8h, holding_mutation ≤2h, impersonation ≤2h | Platform SQL check, `erpx/crates/domain/src/support.rs` |

The Rust support transition implementation was identified as incomplete at G0-001: required waiting/access/in-progress/reopen edges are missing and `resolved → closed` is too permissive. G4 must replace it with the frozen lifecycle before acceptance.
