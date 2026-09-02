# Ecommerce

**Purpose:** Owns e-commerce storefront and order intake.

**Group:** `10-ecommerce` · **Code:** `crates/contexts/ecommerce` · **Proto:** `crates/proto/proto/erpx/ecommerce/service.proto` · **Adapters:** `crates/adapters/ecommerce-sqlx` (where present) · **UI:** `apps/console/src/features/ecommerce` / `crates/ui-domains/*`

**Actors & scopes:** `AuthorityScope::{Platform, Holding, Company, Branch, Subtree}` via `crates/kernel`; RBAC through expiring `role_assignments` (ADR-004) where context is gated.

**Services & RPCs (26):** EcommerceCommandService, EcommerceQueryService
- CreateChannel, ApproveChannel, TransitionChannel, CreateListing, ApproveListing, PublishListing, WithdrawListing, CreateCart, PutCartLine, LockCart, StartCheckout, ReconcileCheckoutRevalidation …

**Invariants:**
- Storefront reads from commercial catalog revisions; orders flow to sales.


**Data owned:** `commercial_catalog_revisions + orders`

**Depends on / depended-by:** depends on `organization`/`identity_access` for scope; collaborators link via `ReferenceQueryService` + Typed Mentions (no direct table joins).

**Referenceable:** yes — `@` + ctx + `/<id>` → `/reference/item/:id` (typed mention). ID example: `@ecommerce/example-28-01-2026-0001`.



**Verification:** `cargo xtask test-affected` (owns `crates/contexts/ecommerce`), `cargo xtask verify` for full gate. Traceability: `Plan/traceability.md`.

ASCII: see `Plan/03-cross-cutting/@-and-ai/ecommerce.md`.
