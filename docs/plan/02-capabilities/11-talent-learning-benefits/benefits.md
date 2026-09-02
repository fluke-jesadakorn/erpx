# Benefits

**Purpose:** Owns benefits plans and enrollments.

**Group:** `11-talent-learning-benefits` · **Code:** `crates/contexts/benefits` · **Proto:** `crates/proto/proto/erpx/benefits/service.proto` · **Adapters:** `crates/adapters/benefits-sqlx` (where present) · **UI:** `apps/console/src/features/benefits` / `crates/ui-domains/*`

**Actors & scopes:** `AuthorityScope::{Platform, Holding, Company, Branch, Subtree}` via `crates/kernel`; RBAC through expiring `role_assignments` (ADR-004) where context is gated.

**Services & RPCs (9):** BenefitsCommandService, BenefitsQueryService
- PublishPlan, OpenEnrollment, AddDependent, ElectCoverage, SubmitProvider, AcknowledgeProvider, ReleasePayroll, GetEnrollment, GetRecord

**Invariants:**
- Enrollment windows are gated; coverage requires dependent validation.


**Data owned:** `benefits enrollments + dependents`

**Depends on / depended-by:** depends on `organization`/`identity_access` for scope; collaborators link via `ReferenceQueryService` + Typed Mentions (no direct table joins).

**Referenceable:** yes — `@` + ctx + `/<id>` → `/reference/item/:id` (typed mention). ID example: `@benefits/example-28-01-2026-0001`.



**Verification:** `cargo xtask test-affected` (owns `crates/contexts/benefits`), `cargo xtask verify` for full gate. Traceability: `docs/plan/traceability.md`.

ASCII: see `docs/plan/03-cross-cutting/@-and-ai/benefits.md`.
