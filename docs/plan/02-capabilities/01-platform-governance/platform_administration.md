# Platform Administration

**Purpose:** Owns holding provisioning, backup/PITR, data-plane, projection orchestration.

**Group:** `01-platform-governance` · **Code:** `crates/contexts/platform_administration` · **Proto:** `crates/proto/proto/erpx/platform_administration/service.proto` · **Adapters:** `crates/adapters/platform_administration-sqlx` (where present) · **UI:** `apps/console/src/features/platform_administration` / `crates/ui-domains/*`

**Actors & scopes:** `AuthorityScope::{Platform, Holding, Company, Branch, Subtree}` via `crates/kernel`; RBAC through expiring `role_assignments` (ADR-004) where context is gated.

**Services & RPCs (65):** PlatformAdministrationCommandService, PlatformAdministrationQueryService, DataPlaneStreamService
- RegisterHolding, TransitionHolding, RegisterConnector, TransitionConnector, IssueAdminContext, RevokeAdminContext, ProvisionHoldingBranch, RetryHoldingBranchProvisioning, SuspendHoldingBranchRoute, BeginHoldingAdminContext, EndHoldingAdminContext, BeginHoldingProvisioning …

**Invariants:**
- Holding provisioning and PITR/backup are platform-only; branch data never leaks across holdings.


**Data owned:** `holding_provisioning + holding_backup_* + platform_pitr_* + worker_heartbeats`

**Depends on / depended-by:** depends on `organization`/`identity_access` for scope; collaborators link via `ReferenceQueryService` + Typed Mentions (no direct table joins).

**Referenceable:** yes — `@` + ctx + `/<id>` → `/reference/item/:id` (typed mention). ID example: `@platform_administration/example-28-01-2026-0001`.



**Verification:** `cargo xtask test-affected` (owns `crates/contexts/platform_administration`), `cargo xtask verify` for full gate. Traceability: `docs/plan/traceability.md`.

ASCII: see `docs/plan/03-cross-cutting/@-and-ai/platform_administration.md`.
