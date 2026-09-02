# Commercial

**Purpose:** Owns Stripe-authoritative catalog/billing/entitlement mirrored locally.

**Group:** `01-platform-governance` · **Code:** `crates/contexts/commercial` · **Proto:** `crates/proto/proto/erpx/commercial/service.proto` · **Adapters:** `crates/adapters/commercial-sqlx` (where present) · **UI:** `apps/console/src/features/commercial` / `crates/ui-domains/*`

**Actors & scopes:** `AuthorityScope::{Platform, Holding, Company, Branch, Subtree}` via `crates/kernel`; RBAC through expiring `role_assignments` (ADR-004) where context is gated.

**Services & RPCs (22):** CommercialCommandService, CommercialQueryService
- AllocateCompanyEntitlement, ActivateCompanyEntitlement, ActivateBranchEntitlement, CreateCheckoutSession, CreateCustomerPortalSession, GrantGraceExtension, CreateCatalogPlan, PublishCatalogPlan, CreateSubscription, ApplyBillingEvent, CreateEntitlement, ApplyEntitlementChange …

**Invariants:**
- Billing state is Stripe-authoritative; local state mirrors Stripe events.


**Data owned:** `commercial_catalog_revisions + billing_lifecycles + entitlement_allocation_books + stripe_event_inbox`

**Depends on / depended-by:** depends on `organization`/`identity_access` for scope; collaborators link via `ReferenceQueryService` + Typed Mentions (no direct table joins).

**Referenceable:** yes — `@` + ctx + `/<id>` → `/reference/item/:id` (typed mention). ID example: `@commercial/example-28-01-2026-0001`.



**Verification:** `cargo xtask test-affected` (owns `crates/contexts/commercial`), `cargo xtask verify` for full gate. Traceability: `docs/plan/traceability.md`.

ASCII: see `docs/plan/03-cross-cutting/@-and-ai/commercial.md`.
