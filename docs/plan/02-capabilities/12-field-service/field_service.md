# Field Service

**Purpose:** Owns field service orders and agreements.

**Group:** `12-field-service` · **Code:** `crates/contexts/field_service` · **Proto:** `crates/proto/proto/erpx/field_service/service.proto` · **Adapters:** `crates/adapters/field_service-sqlx` (where present) · **UI:** `apps/console/src/features/field_service` / `crates/ui-domains/*`

**Actors & scopes:** `AuthorityScope::{Platform, Holding, Company, Branch, Subtree}` via `crates/kernel`; RBAC through expiring `role_assignments` (ADR-004) where context is gated.

**Services & RPCs (12):** FieldServiceCommandService, FieldServiceQueryService
- ActivateAgreement, CreateOrder, ScheduleOrder, ReservePart, DispatchOrder, StartVisit, RecordException, IssuePart, CompleteVisit, ReleaseBilling, GetOrder, GetRecord

**Invariants:**
- Service orders require human decision evidence for activation.


**Data owned:** `field_service orders + agreements`

**Depends on / depended-by:** depends on `organization`/`identity_access` for scope; collaborators link via `ReferenceQueryService` + Typed Mentions (no direct table joins).

**Referenceable:** yes — `@` + ctx + `/<id>` → `/reference/item/:id` (typed mention). ID example: `@field_service/example-28-01-2026-0001`.



**Verification:** `cargo xtask test-affected` (owns `crates/contexts/field_service`), `cargo xtask verify` for full gate. Traceability: `docs/plan/traceability.md`.

ASCII: see `docs/plan/03-cross-cutting/@-and-ai/field_service.md`.
