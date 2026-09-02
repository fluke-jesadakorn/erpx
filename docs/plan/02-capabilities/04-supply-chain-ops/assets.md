# Assets

**Purpose:** Owns asset lifecycle.

**Group:** `04-supply-chain-ops` · **Code:** `crates/contexts/assets` · **Proto:** `crates/proto/proto/erpx/assets/service.proto` · **Adapters:** `crates/adapters/assets-sqlx` (where present) · **UI:** `apps/console/src/features/assets` / `crates/ui-domains/*`

**Actors & scopes:** `AuthorityScope::{Platform, Holding, Company, Branch, Subtree}` via `crates/kernel`; RBAC through expiring `role_assignments` (ADR-004) where context is gated.

**Services & RPCs (8):** AssetsCommandService, AssetsQueryService
- Create, PlaceInService, MarkDegraded, BeginMaintenance, ReturnToService, Retire, Get, List

**Invariants:**
- Lifecycle: draft → in-service → degraded/under-maintenance → retired — illegal transitions rejected.


**Data owned:** `assets_* + maintenance link`

**Depends on / depended-by:** depends on `organization`/`identity_access` for scope; collaborators link via `ReferenceQueryService` + Typed Mentions (no direct table joins).

**Referenceable:** yes — `@` + ctx + `/<id>` → `/reference/item/:id` (typed mention). ID example: `@assets/example-28-01-2026-0001`.



**Verification:** `cargo xtask test-affected` (owns `crates/contexts/assets`), `cargo xtask verify` for full gate. Traceability: `docs/plan/traceability.md`.

ASCII: see `docs/plan/03-cross-cutting/@-and-ai/assets.md`.
