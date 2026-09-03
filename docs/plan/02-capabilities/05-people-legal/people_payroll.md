# People Payroll

**Purpose:** Owns HR and payroll projections.

**Group:** `05-people-legal` · **Code:** `crates/contexts/people_payroll` · **Proto:** `crates/proto/proto/erpx/people_payroll/service.proto` · **Adapters:** `crates/adapters/people_payroll-sqlx` (where present) · **UI:** `apps/console/src/features/people_payroll` / `crates/ui-domains/*`

**Actors & scopes:** `AuthorityScope::{Platform, Holding, Company, Branch, Subtree}` via `crates/kernel`; RBAC through expiring `role_assignments` (ADR-004) where context is gated.

**Services & RPCs (16):** PeoplePayrollCommandService, PeoplePayrollQueryService
- HireEmployee, TransitionEmployee, CreateLeaveRequest, TransitionLeaveRequest, CreateExpenseClaim, TransitionExpenseClaim, CreatePayrollRun, TransitionPayrollRun, GetEmployee, ListEmployees, GetLeaveRequest, ListLeaveRequests …

**Invariants:**
- HR assignments are time-bounded; payroll reads from HR projections.


**Data owned:** `hr_* + payroll projections`

**Depends on / depended-by:** depends on `organization`/`identity_access` for scope; collaborators link via `ReferenceQueryService` + Typed Mentions (no direct table joins).

**Referenceable:** yes — `@` + ctx + `/<id>` → `/reference/item/:id` (typed mention). ID example: `@people_payroll/example-28-01-2026-0001`.



## Social surface (paradigm: `docs/plan/04-social-paradigm.md`)

- **Post template:** `{leave/expense/run} · {person} · {state}` — amounts masked to authorized roles.
- **Hashtags:** `#leave #expense #payroll` (restricted audience).
- **Follow:** own requests + team runs; **Lane:** pending decisions for `hr-approver`.

**Verification:** `cargo xtask test-affected` (owns `crates/contexts/people_payroll`), `cargo xtask verify` for full gate. Traceability: `docs/plan/traceability.md`.

ASCII: see `docs/plan/03-cross-cutting/@-and-ai/people_payroll.md`.
