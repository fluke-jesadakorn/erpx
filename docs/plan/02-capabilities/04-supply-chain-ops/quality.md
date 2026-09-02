# Quality

**Purpose:** Owns quality controls gating operations.

**Group:** `04-supply-chain-ops` · **Code:** `crates/contexts/quality` · **Proto:** `crates/proto/proto/erpx/quality/service.proto` · **Adapters:** `crates/adapters/quality-sqlx` (where present) · **UI:** `apps/console/src/features/quality` / `crates/ui-domains/*`

**Actors & scopes:** `AuthorityScope::{Platform, Holding, Company, Branch, Subtree}` via `crates/kernel`; RBAC through expiring `role_assignments` (ADR-004) where context is gated.

**Services & RPCs (7):** QualityCommandService, QualityQueryService
- Create, Start, RecordResult, Release, Cancel, Get, List

**Invariants:**
- Quality checks gate operations completion.


**Data owned:** `quality_*`

**Depends on / depended-by:** depends on `organization`/`identity_access` for scope; collaborators link via `ReferenceQueryService` + Typed Mentions (no direct table joins).

**Referenceable:** yes — `@` + ctx + `/<id>` → `/reference/item/:id` (typed mention). ID example: `@quality/example-28-01-2026-0001`.



**Verification:** `cargo xtask test-affected` (owns `crates/contexts/quality`), `cargo xtask verify` for full gate. Traceability: `docs/plan/traceability.md`.

ASCII: see `docs/plan/03-cross-cutting/@-and-ai/quality.md`.
