# Projects

**Purpose:** Owns projects/WBS.

**Group:** `09-projects` · **Code:** `crates/contexts/projects` · **Proto:** `crates/proto/proto/erpx/projects/service.proto` · **Adapters:** `crates/adapters/projects-sqlx` (where present) · **UI:** `apps/console/src/features/projects` / `crates/ui-domains/*`

**Actors & scopes:** `AuthorityScope::{Platform, Holding, Company, Branch, Subtree}` via `crates/kernel`; RBAC through expiring `role_assignments` (ADR-004) where context is gated.

**Services & RPCs (23):** ProjectsCommandService, ProjectsQueryService
- CreateProject, TransitionProject, AddNode, MoveNode, TransitionNode, AttachNodeReference, AddNodeAttachment, RecordNodeTime, RecordNodeCost, ProposeDependency, DecideDependency, CancelDependency …

**Invariants:**
- Projects own WBS and resource assignments; billing via commercial.


**Data owned:** `projects WBS`

**Depends on / depended-by:** depends on `organization`/`identity_access` for scope; collaborators link via `ReferenceQueryService` + Typed Mentions (no direct table joins).

**Referenceable:** yes — `@` + ctx + `/<id>` → `/reference/item/:id` (typed mention). ID example: `@projects/example-28-01-2026-0001`.



## Social surface (paradigm: `docs/plan/04-social-paradigm.md`)

- **Post template:** `PROJ-… · {wbs-node} · {progress} · {state}` — WBS nodes post progress; the chain is a followable story.
- **Hashtags:** `#project #milestone #blocked` (holding scope).
- **Follow:** projects/nodes; **Lane:** blocked nodes + dependency decisions for `pm`.

**Verification:** `cargo xtask test-affected` (owns `crates/contexts/projects`), `cargo xtask verify` for full gate. Traceability: `docs/plan/traceability.md`.

ASCII: see `docs/plan/03-cross-cutting/@-and-ai/projects.md`.
