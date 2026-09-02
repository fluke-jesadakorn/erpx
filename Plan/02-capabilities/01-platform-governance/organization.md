# Organization

**Purpose:** Owns Holdings/Companies/Branches and sharing — the isolation spine.

**Group:** `01-platform-governance` · **Code:** `crates/contexts/organization` · **Proto:** `crates/proto/proto/erpx/organization/service.proto` · **Adapters:** `crates/adapters/organization-sqlx` (where present) · **UI:** `apps/console/src/features/organization` / `crates/ui-domains/*`

**Actors & scopes:** `AuthorityScope::{Platform, Holding, Company, Branch, Subtree}` via `crates/kernel`; RBAC through expiring `role_assignments` (ADR-004) where context is gated.

**Services & RPCs (9):** OrganizationCommandService, OrganizationQueryService
- CreateNode, RenameNode, ChangeNodeCode, ReparentNode, SuspendNode, ActivateNode, ArchiveNode, GetNode, ListNodes

**Invariants:**
- Holdings/companies/branches are the isolation spine (ADR-001/013); sharing is explicit.


**Data owned:** `holding_registry + organization nodes + customer_holding_bindings`

**Depends on / depended-by:** depends on `organization`/`identity_access` for scope; collaborators link via `ReferenceQueryService` + Typed Mentions (no direct table joins).

**Referenceable:** yes — `@` + ctx + `/<id>` → `/reference/item/:id` (typed mention). ID example: `@organization/example-28-01-2026-0001`.



**Verification:** `cargo xtask test-affected` (owns `crates/contexts/organization`), `cargo xtask verify` for full gate. Traceability: `Plan/traceability.md`.

ASCII: see `Plan/03-cross-cutting/@-and-ai/organization.md`.
