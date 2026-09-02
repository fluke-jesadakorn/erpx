# Brand

**Purpose:** Owns brand drafts with versioned publish.

**Group:** `01-platform-governance` · **Code:** `crates/contexts/brand` · **Proto:** `crates/proto/proto/erpx/brand/service.proto` · **Adapters:** `crates/adapters/brand-sqlx` (where present) · **UI:** `apps/console/src/features/brand` / `crates/ui-domains/*`

**Actors & scopes:** `AuthorityScope::{Platform, Holding, Company, Branch, Subtree}` via `crates/kernel`; RBAC through expiring `role_assignments` (ADR-004) where context is gated.

**Services & RPCs (8):** BrandCommandService, BrandQueryService
- CreateDraft, SaveDraft, PublishDraft, Rollback, SetCategoryLocks, GetDraft, GetResolved, ListHistory

**Invariants:**
- Brand drafts are versioned with category locks; publish is atomic.


**Data owned:** `brand_drafts + category locks`

**Depends on / depended-by:** depends on `organization`/`identity_access` for scope; collaborators link via `ReferenceQueryService` + Typed Mentions (no direct table joins).

**Referenceable:** yes — `@` + ctx + `/<id>` → `/reference/item/:id` (typed mention). ID example: `@brand/example-28-01-2026-0001`.



**Verification:** `cargo xtask test-affected` (owns `crates/contexts/brand`), `cargo xtask verify` for full gate. Traceability: `docs/plan/traceability.md`.

ASCII: see `docs/plan/03-cross-cutting/@-and-ai/brand.md`.
