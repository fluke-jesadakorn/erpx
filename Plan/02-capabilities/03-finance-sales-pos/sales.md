# Sales

**Purpose:** Owns quotes/orders and posting to finance.

**Group:** `03-finance-sales-pos` · **Code:** `crates/contexts/sales` · **Proto:** `crates/proto/proto/erpx/sales/service.proto` · **Adapters:** `crates/adapters/sales-sqlx` (where present) · **UI:** `apps/console/src/features/sales` / `crates/ui-domains/*`

**Actors & scopes:** `AuthorityScope::{Platform, Holding, Company, Branch, Subtree}` via `crates/kernel`; RBAC through expiring `role_assignments` (ADR-004) where context is gated.

**Services & RPCs (13):** SalesCommandService, SalesQueryService
- CreateQuote, SendQuote, AcceptQuote, RejectQuote, ExpireQuote, CreateSalesOrder, ConfirmSalesOrder, MarkSalesOrderPartiallyFulfilled, MarkSalesOrderFulfilled, CancelSalesOrder, GetQuote, GetSalesOrder …

**Invariants:**
- Quote → order → posting chain validated in `sales` domain before finance posting.
- No direct finance writes from sales outside posting application.

**Data owned:** `sales projection + finance posting link`

**Depends on / depended-by:** depends on `organization`/`identity_access` for scope; collaborators link via `ReferenceQueryService` + Typed Mentions (no direct table joins).

**Referenceable:** yes — `@` + ctx + `/<id>` → `/reference/item/:id` (typed mention). ID example: `@sales/example-28-01-2026-0001`.



**Verification:** `cargo xtask test-affected` (owns `crates/contexts/sales`), `cargo xtask verify` for full gate. Traceability: `Plan/traceability.md`.

ASCII: see `Plan/03-cross-cutting/@-and-ai/sales.md`.
