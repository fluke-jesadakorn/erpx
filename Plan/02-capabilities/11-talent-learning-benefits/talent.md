# Talent

**Purpose:** Owns recruiting & talent pipelines.

**Group:** `11-talent-learning-benefits` · **Code:** `crates/contexts/talent` · **Proto:** `crates/proto/proto/erpx/talent/service.proto` · **Adapters:** `crates/adapters/talent-sqlx` (where present) · **UI:** `apps/console/src/features/talent` / `crates/ui-domains/*`

**Actors & scopes:** `AuthorityScope::{Platform, Holding, Company, Branch, Subtree}` via `crates/kernel`; RBAC through expiring `role_assignments` (ADR-004) where context is gated.

**Services & RPCs (28):** TalentCommandService, TalentQueryService
- CreateRequisition, ApproveRequisition, TransitionRequisition, CreateCandidateProfile, RequestCandidateErasure, CompleteCandidateErasure, SetCandidateLegalHold, SubmitApplication, AddApplicationInterview, SelectApplication, WithdrawApplication, CreateOffer …

**Invariants:**
- Talent pipelines are auditable; decisions carry evidence.


**Data owned:** `talent pipelines`

**Depends on / depended-by:** depends on `organization`/`identity_access` for scope; collaborators link via `ReferenceQueryService` + Typed Mentions (no direct table joins).

**Referenceable:** yes — `@` + ctx + `/<id>` → `/reference/item/:id` (typed mention). ID example: `@talent/example-28-01-2026-0001`.



**Verification:** `cargo xtask test-affected` (owns `crates/contexts/talent`), `cargo xtask verify` for full gate. Traceability: `Plan/traceability.md`.

ASCII: see `Plan/03-cross-cutting/@-and-ai/talent.md`.
