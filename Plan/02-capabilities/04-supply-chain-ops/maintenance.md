# Maintenance

**Purpose:** Owns maintenance work orders.

**Group:** `04-supply-chain-ops` · **Code:** `crates/contexts/maintenance` · **Proto:** `crates/proto/proto/erpx/maintenance/service.proto` · **Adapters:** `crates/adapters/maintenance-sqlx` (where present) · **UI:** `apps/console/src/features/maintenance` / `crates/ui-domains/*`

**Actors & scopes:** `AuthorityScope::{Platform, Holding, Company, Branch, Subtree}` via `crates/kernel`; RBAC through expiring `role_assignments` (ADR-004) where context is gated.

**Services & RPCs (10):** MaintenanceCommandService, MaintenanceQueryService
- Open, Schedule, Start, Block, Resume, Complete, Cancel, RecordDowntime, Get, List

**Invariants:**
- Work orders bound to assets; completion requires evidence.


**Data owned:** `maintenance work orders`

**Depends on / depended-by:** depends on `organization`/`identity_access` for scope; collaborators link via `ReferenceQueryService` + Typed Mentions (no direct table joins).

**Referenceable:** yes — `@` + ctx + `/<id>` → `/reference/item/:id` (typed mention). ID example: `@maintenance/example-28-01-2026-0001`.



**Verification:** `cargo xtask test-affected` (owns `crates/contexts/maintenance`), `cargo xtask verify` for full gate. Traceability: `Plan/traceability.md`.

ASCII: see `Plan/03-cross-cutting/@-and-ai/maintenance.md`.
