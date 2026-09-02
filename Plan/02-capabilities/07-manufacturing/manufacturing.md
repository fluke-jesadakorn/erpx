# Manufacturing

**Purpose:** Owns manufacturing orders and execution.

**Group:** `07-manufacturing` · **Code:** `crates/contexts/manufacturing` · **Proto:** `crates/proto/proto/erpx/manufacturing/service.proto` · **Adapters:** `crates/adapters/manufacturing-sqlx` (where present) · **UI:** `apps/console/src/features/manufacturing` / `crates/ui-domains/*`

**Actors & scopes:** `AuthorityScope::{Platform, Holding, Company, Branch, Subtree}` via `crates/kernel`; RBAC through expiring `role_assignments` (ADR-004) where context is gated.

**Services & RPCs (18):** ManufacturingCommandService, ManufacturingQueryService
- PublishSpecification, CreateOrder, ReleaseOrder, ReserveMaterial, StartOrder, IssueMaterial, ReportLabor, ReportResult, ApplyQualityDisposition, RecordCost, CompleteOrder, CloseOrder …

**Invariants:**
- Manufacturing orders consume planning + inventory via contracts.


**Data owned:** `manufacturing orders`

**Depends on / depended-by:** depends on `organization`/`identity_access` for scope; collaborators link via `ReferenceQueryService` + Typed Mentions (no direct table joins).

**Referenceable:** yes — `@` + ctx + `/<id>` → `/reference/item/:id` (typed mention). ID example: `@manufacturing/example-28-01-2026-0001`.



**Verification:** `cargo xtask test-affected` (owns `crates/contexts/manufacturing`), `cargo xtask verify` for full gate. Traceability: `Plan/traceability.md`.

ASCII: see `Plan/03-cross-cutting/@-and-ai/manufacturing.md`.
