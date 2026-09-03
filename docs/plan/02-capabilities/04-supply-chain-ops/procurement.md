# Procurement

**Purpose:** Owns procurement lifecycle.

**Group:** `04-supply-chain-ops` · **Code:** `crates/contexts/procurement` · **Proto:** `crates/proto/proto/erpx/procurement/service.proto` · **Adapters:** `crates/adapters/procurement-sqlx` (where present) · **UI:** `apps/console/src/features/procurement` / `crates/ui-domains/*`

**Actors & scopes:** `AuthorityScope::{Platform, Holding, Company, Branch, Subtree}` via `crates/kernel`; RBAC through expiring `role_assignments` (ADR-004) where context is gated.

**Services & RPCs (16):** ProcurementCommandService, ProcurementQueryService
- Create, Release, Cancel, CreateRequisition, SubmitRequisition, ApproveRequisition, RejectRequisition, CancelRequisition, CreateGoodsReceipt, AcceptGoodsReceipt, RejectGoodsReceipt, CancelGoodsReceipt …

**Invariants:**
- Procurement references inventory/operations via typed contracts, not shared tables.


**Data owned:** `procurement_*`

**Depends on / depended-by:** depends on `organization`/`identity_access` for scope; collaborators link via `ReferenceQueryService` + Typed Mentions (no direct table joins).

**Referenceable:** yes — `@` + ctx + `/<id>` → `/reference/item/:id` (typed mention). ID example: `@procurement/example-28-01-2026-0001`.



## Social surface (paradigm: `docs/plan/04-social-paradigm.md`)

- **Post template:** `REQ-… · {lines} · {total} · {state}` — requisitions invite discussion before approval.
- **Hashtags:** `#procurement #po #receipt` (holding scope).
- **Follow:** requisitions/POs; **Lane:** pending approvals for `procurement-approver` (flow: `_journeys.md` Journey 2).

**Verification:** `cargo xtask test-affected` (owns `crates/contexts/procurement`), `cargo xtask verify` for full gate. Traceability: `docs/plan/traceability.md`.

ASCII: see `docs/plan/03-cross-cutting/@-and-ai/procurement.md`.
