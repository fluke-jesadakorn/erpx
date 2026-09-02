# Agent

**Purpose:** Owns agents, pools, assignments, private sessions, field suggestions.

**Group:** `13-ai-collaboration` · **Code:** `crates/contexts/agent` · **Proto:** `crates/proto/proto/erpx/agent/service.proto` · **Adapters:** `crates/adapters/agent-sqlx` (where present) · **UI:** `apps/console/src/features/agent` / `crates/ui-domains/*`

**Actors & scopes:** `AuthorityScope::{Platform, Holding, Company, Branch, Subtree}` via `crates/kernel`; RBAC through expiring `role_assignments` (ADR-004) where context is gated.

**Services & RPCs (30):** AgentOrchestrationCommandService, AgentOrchestrationQueryService
- RegisterAgent, AddAgentVersion, ApproveAgentVersion, SuspendAgent, RetireAgent, StartAssignment, DelegateAssignment, RetryAssignment, CancelAssignment, RegisterPool, ReplacePoolMembers, RoutePool …

**Invariants:**
- Agent versions require approval; assignments are routable and auditable with invocation evidence.


**Data owned:** `agent definitions/assignments/pools/sessions + field suggestions`

**Depends on / depended-by:** depends on `organization`/`identity_access` for scope; collaborators link via `ReferenceQueryService` + Typed Mentions (no direct table joins).

**Referenceable:** yes — `@` + ctx + `/<id>` → `/reference/item/:id` (typed mention). ID example: `@agent/example-28-01-2026-0001`.



**Verification:** `cargo xtask test-affected` (owns `crates/contexts/agent`), `cargo xtask verify` for full gate. Traceability: `docs/plan/traceability.md`.

ASCII: see `docs/plan/03-cross-cutting/@-and-ai/agent.md`.
