# Customer 360

**Purpose:** Federated customer read projection — no write ownership.

**Group:** `02-customer-support` · **Code:** `crates/contexts/customer_360` · **Proto:** `crates/proto/proto/erpx/customer_360/service.proto` · **Adapters:** `crates/adapters/customer_360-sqlx` (where present) · **UI:** `apps/console/src/features/customer_360` / `crates/ui-domains/*`

**Actors & scopes:** `AuthorityScope::{Platform, Holding, Company, Branch, Subtree}` via `crates/kernel`; RBAC through expiring `role_assignments` (ADR-004) where context is gated.

**Services & RPCs (3):** Customer360CommandService, Customer360QueryService
- AddCustomerNote, GetCustomer360, GetCustomer360Detail

**Invariants:**
- Customer 360 is a federated read projection, not a write owner.


**Data owned:** `federated read (no owned tables)`

**Depends on / depended-by:** depends on `organization`/`identity_access` for scope; collaborators link via `ReferenceQueryService` + Typed Mentions (no direct table joins).

**Referenceable:** yes — `@` + ctx + `/<id>` → `/reference/item/:id` (typed mention). ID example: `@customer_360/example-28-01-2026-0001`.

**Gaps:** code-thin area (e.g., no service yet or projection-only); business intent preserved, implementation follows.

**Verification:** `cargo xtask test-affected` (owns `crates/contexts/customer_360`), `cargo xtask verify` for full gate. Traceability: `Plan/traceability.md`.

ASCII: see `Plan/03-cross-cutting/@-and-ai/customer_360.md`.
