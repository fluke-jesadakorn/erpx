# Learning

**Purpose:** Owns learning enrollments.

**Group:** `11-talent-learning-benefits` · **Code:** `crates/contexts/learning` · **Proto:** `crates/proto/proto/erpx/learning/service.proto` · **Adapters:** `crates/adapters/learning-sqlx` (where present) · **UI:** `apps/console/src/features/learning` / `crates/ui-domains/*`

**Actors & scopes:** `AuthorityScope::{Platform, Holding, Company, Branch, Subtree}` via `crates/kernel`; RBAC through expiring `role_assignments` (ADR-004) where context is gated.

**Services & RPCs (9):** LearningCommandService, LearningQueryService
- PublishCourse, RetireCourse, AssignCourse, RecordAttendance, RecordProviderResult, WaiveEnrollment, CompleteEnrollment, GetEnrollment, GetRecord

**Invariants:**
- Learning enrollments are idempotent; completion drives benefits eligibility.


**Data owned:** `learning enrollments`

**Depends on / depended-by:** depends on `organization`/`identity_access` for scope; collaborators link via `ReferenceQueryService` + Typed Mentions (no direct table joins).

**Referenceable:** yes — `@` + ctx + `/<id>` → `/reference/item/:id` (typed mention). ID example: `@learning/example-28-01-2026-0001`.



**Verification:** `cargo xtask test-affected` (owns `crates/contexts/learning`), `cargo xtask verify` for full gate. Traceability: `Plan/traceability.md`.

ASCII: see `Plan/03-cross-cutting/@-and-ai/learning.md`.
