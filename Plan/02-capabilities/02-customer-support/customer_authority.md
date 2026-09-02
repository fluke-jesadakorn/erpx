# Customer Authority

**Purpose:** Owns customer authority bindings distinct from identity.

**Group:** `02-customer-support` · **Code:** `crates/contexts/customer_authority` · **Proto:** `crates/proto/proto/erpx/customer_authority/service.proto` · **Adapters:** `crates/adapters/customer_authority-sqlx` (where present) · **UI:** `apps/console/src/features/customer_authority` / `crates/ui-domains/*`

**Actors & scopes:** `AuthorityScope::{Platform, Holding, Company, Branch, Subtree}` via `crates/kernel`; RBAC through expiring `role_assignments` (ADR-004) where context is gated.

**Services & RPCs (9):** CustomerAuthorityCommandService, CustomerAuthorityQueryService
- Create, UpdateIdentity, ChangeStatus, LinkAuthority, UnlinkAuthority, Merge, Get, List, GetCustomerAccessAudit

**Invariants:**
- Customer authority is distinct from identity; bindings are explicit.


**Data owned:** `customer_scope_bindings + object_access_bindings`

**Depends on / depended-by:** depends on `organization`/`identity_access` for scope; collaborators link via `ReferenceQueryService` + Typed Mentions (no direct table joins).

**Referenceable:** yes — `@` + ctx + `/<id>` → `/reference/item/:id` (typed mention). ID example: `@customer_authority/example-28-01-2026-0001`.



**Verification:** `cargo xtask test-affected` (owns `crates/contexts/customer_authority`), `cargo xtask verify` for full gate. Traceability: `Plan/traceability.md`.

ASCII: see `Plan/03-cross-cutting/@-and-ai/customer_authority.md`.
