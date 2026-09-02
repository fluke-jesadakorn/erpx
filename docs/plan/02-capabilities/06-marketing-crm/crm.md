# Crm

**Purpose:** Owns CRM opportunities/accounts.

**Group:** `06-marketing-crm` · **Code:** `crates/contexts/crm` · **Proto:** `crates/proto/proto/erpx/crm/service.proto` · **Adapters:** `crates/adapters/crm-sqlx` (where present) · **UI:** `apps/console/src/features/crm` / `crates/ui-domains/*`

**Actors & scopes:** `AuthorityScope::{Platform, Holding, Company, Branch, Subtree}` via `crates/kernel`; RBAC through expiring `role_assignments` (ADR-004) where context is gated.

**Services & RPCs (10):** CrmCommandService, CrmQueryService
- SaveCrmHandoff, SaveCrmOpportunity, RecordCrmAttribution, AcceptMarketingLead, CreateOpportunity, TransitionOpportunity, ListCrmReadRows, GetCrmLead, GetOpportunity, ListOpportunities

**Invariants:**
- CRM owns opportunity/account lifecycle independent of sales ordering.


**Data owned:** `crm_*`

**Depends on / depended-by:** depends on `organization`/`identity_access` for scope; collaborators link via `ReferenceQueryService` + Typed Mentions (no direct table joins).

**Referenceable:** yes — `@` + ctx + `/<id>` → `/reference/item/:id` (typed mention). ID example: `@crm/example-28-01-2026-0001`.



**Verification:** `cargo xtask test-affected` (owns `crates/contexts/crm`), `cargo xtask verify` for full gate. Traceability: `docs/plan/traceability.md`.

ASCII: see `docs/plan/03-cross-cutting/@-and-ai/crm.md`.
