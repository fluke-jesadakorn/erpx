# Planning

**Purpose:** Owns supply & demand planning projections.

**Group:** `08-planning` · **Code:** `crates/contexts/planning` · **Proto:** `crates/proto/proto/erpx/planning/service.proto` · **Adapters:** `crates/adapters/planning-sqlx` (where present) · **UI:** `apps/console/src/features/planning` / `crates/ui-domains/*`

**Actors & scopes:** `AuthorityScope::{Platform, Holding, Company, Branch, Subtree}` via `crates/kernel`; RBAC through expiring `role_assignments` (ADR-004) where context is gated.

**Services & RPCs (14):** PlanningCommandService, PlanningQueryService
- CreateScenario, FreezeScenario, RunScenario, OverrideRecommendation, ApproveRecommendation, RejectRecommendation, ReleaseRecommendation, GetScenario, GetRecommendation, GetRun, ListRecommendations, ListShortages …

**Invariants:**
- Supply & demand planning is projection-driven, not a write owner of stock.


**Data owned:** `planning projections`

**Depends on / depended-by:** depends on `organization`/`identity_access` for scope; collaborators link via `ReferenceQueryService` + Typed Mentions (no direct table joins).

**Referenceable:** yes — `@` + ctx + `/<id>` → `/reference/item/:id` (typed mention). ID example: `@planning/example-28-01-2026-0001`.



## Social surface (paradigm: `docs/plan/04-social-paradigm.md`)

- **Post template:** `SCN-… · {horizon} · {shortages} · {state}` — projections discussed, never edited in feed.
- **Hashtags:** `#plan #shortage #scenario` (holding scope).
- **Follow:** scenarios; **Lane:** released recommendations for `planner`.

**Verification:** `cargo xtask test-affected` (owns `crates/contexts/planning`), `cargo xtask verify` for full gate. Traceability: `docs/plan/traceability.md`.

ASCII: see `docs/plan/03-cross-cutting/@-and-ai/planning.md`.
