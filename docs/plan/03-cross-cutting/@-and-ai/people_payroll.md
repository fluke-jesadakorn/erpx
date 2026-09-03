# People Payroll — Deep `@` + AI (Production)

**Group:** `05-people-legal` · **Code:** `crates/contexts/people_payroll` · **Proto:** `crates/proto/proto/erpx/people_payroll/service.proto` (`PeoplePayrollCommandService, PeoplePayrollQueryService` · 16 RPCs) · **Adapters:** `crates/adapters/people_payroll-sqlx` · **UI:** `apps/console/src/features/people_payroll` / `crates/ui-domains/*` · **Migrations:** `crates/migration/migrations/000*_*.sql`

**Scope:** Extends `docs/plan/02-capabilities/05-people-legal/people_payroll.md`. Central polymorphic `@` contract in `docs/plan/01-vision-and-scope.md` applies verbatim — see `_shared-contract.md` for @ tokenizer, AI lifecycle, Forms, Verify, Spine, ASCII frame.

## Services & RPCs (16)

- **Services:** PeoplePayrollCommandService, PeoplePayrollQueryService
- **RPCs:** ** HireEmployee, TransitionEmployee, CreateLeaveRequest, TransitionLeaveRequest, CreateExpenseClaim, TransitionExpenseClaim, CreatePayrollRun, TransitionPayrollRun, GetEmployee, ListEmployees, GetLeaveRequest, ListLeaveRequests, GetExpenseClaim, ListExpenseClaims, GetPayrollRun, ListPayrollRuns

| RPC | Service | Data touch |
|-----|---------|------------|
| `** HireEmployee` | Command/Query | `people_payroll` aggregate + outbox |
| `TransitionEmployee` | Command/Query | `people_payroll` aggregate + outbox |
| `CreateLeaveRequest` | Command/Query | `people_payroll` aggregate + outbox |
| `TransitionLeaveRequest` | Command/Query | `people_payroll` aggregate + outbox |
| `CreateExpenseClaim` | Command/Query | `people_payroll` aggregate + outbox |
| `TransitionExpenseClaim` | Command/Query | `people_payroll` aggregate + outbox |
| `CreatePayrollRun` | Command/Query | `people_payroll` aggregate + outbox |
| `TransitionPayrollRun` | Command/Query | `people_payroll` aggregate + outbox |
| `GetEmployee` | Command/Query | `people_payroll` projection |
| `ListEmployees` | Command/Query | `people_payroll` projection |
| `GetLeaveRequest` | Command/Query | `people_payroll` projection |
| `ListLeaveRequests` | Command/Query | `people_payroll` projection |

## Invariants (domain — keep)

- HR assignments are time-bounded; payroll reads from HR projections.

Generic 3 (holding-isolation / time-bounded / append-only) — see `_shared-contract.md`.

## Data owned (delta)

- Write: `people_payroll` aggregate + `outbox_events` (CQRS, ADR-002) in holding/branch pair
- Read: rebuildable projection for search; `hr_* + payroll projections` (light `people_payroll.md:16` canonical)
- Reference graph: see `_shared-contract.md` `record_reference_edges` definition

## Editable fields & schema binding

- **This module:** ** no owner-published schema — OwnerUnsupported (spec-ahead candidate)
- Binding: see `_shared-contract.md` (`MAX_FIELD_PATCH_OPERATIONS=64`, `StableFieldRef`, etc.)

## Business flows (unique)

1. **Happy:** Hire → assignment → payroll run → payslip — creates typed edges (e.g., `@sales/…`, `@finance/…`) and is queryable via `search`/`backlinks`.
2. **Exception:** see original deep — validates invariants + degraded `Stale/Degraded`.
3. **AI-assisted:** see `_shared-contract.md` (pins ≥3 refs → ghost → Apply/Conflicted).

## Feed mapping (paradigm: `docs/plan/04-social-paradigm.md`)

- **Emit:** requests→posts to requester + approver; `TransitionPayrollRun`→lane (`hr-approver`); amounts masked by role.
- **Comments:** requester/approver only; no salary figures in feed text.

## Route & UI delta

- **Route:** `/people + /work` + `/work + /reference/item/:id`
- Shared frame: see `_shared-contract.md` ASCII (title PEOPLE PAYROLL, list people_payroll-001/002)

## Traceability

- **Proto:** `crates/proto/proto/erpx/people_payroll/service.proto` + `reference/service.proto`
- **Domain:** `crates/contexts/people_payroll/src/domain.rs`
- **Adapters:** `people_payroll-sqlx`
- **UI:** `apps/console/src/features/people_payroll`
- **Verify:** see `_shared-contract.md` (Q15=C E2E + `cargo xtask test-affected` for `people_payroll`)

## Revision

- Code SHA: fill at PR time (Q18=A). See `docs/plan/MAP.md` for flat index.
