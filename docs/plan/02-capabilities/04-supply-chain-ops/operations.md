# Operations

**Purpose:** Owns operations execution and completion.

**Group:** `04-supply-chain-ops` · **Code:** `crates/contexts/operations` · **Proto:** `crates/proto/proto/erpx/operations/service.proto` · **Adapters:** `crates/adapters/operations-sqlx` (where present) · **UI:** `apps/console/src/features/operations` / `crates/ui-domains/*`

**Actors & scopes:** `AuthorityScope::{Platform, Holding, Company, Branch, Subtree}` via `crates/kernel`; RBAC through expiring `role_assignments` (ADR-004) where context is gated.

**Services & RPCs (11):** OperationsCommandService, OperationsQueryService
- ReassignException, AcknowledgeException, EscalateException, DecideApproval, ResolveException, CloseException, PublishWorkQueuePolicy, GetException, ListExceptionQueue, ListWorkQueuePolicies, GetSummary

**Invariants:**
- Operations aggregates own completion; cross-context updates via domain events.


**Data owned:** `operations_* + completion evidence`

**Depends on / depended-by:** depends on `organization`/`identity_access` for scope; collaborators link via `ReferenceQueryService` + Typed Mentions (no direct table joins).

**Referenceable:** yes — `@` + ctx + `/<id>` → `/reference/item/:id` (typed mention). ID example: `@operations/example-28-01-2026-0001`.



**Verification:** `cargo xtask test-affected` (owns `crates/contexts/operations`), `cargo xtask verify` for full gate. Traceability: `docs/plan/traceability.md`.

ASCII: see `docs/plan/03-cross-cutting/@-and-ai/operations.md`.
