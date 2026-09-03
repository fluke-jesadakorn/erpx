# Pos

**Purpose:** Owns point-of-sale sessions and refunds.

**Group:** `03-finance-sales-pos` · **Code:** `crates/contexts/pos` · **Proto:** `crates/proto/proto/erpx/pos/service.proto` · **Adapters:** `crates/adapters/pos-sqlx` (where present) · **UI:** `apps/console/src/features/pos` / `crates/ui-domains/*`

**Actors & scopes:** `AuthorityScope::{Platform, Holding, Company, Branch, Subtree}` via `crates/kernel`; RBAC through expiring `role_assignments` (ADR-004) where context is gated.

**Services & RPCs (16):** PointOfSaleCommandService, PointOfSaleQueryService
- CreateSale, OpenSale, HoldSale, ResumeSale, RecordPayment, CompleteSale, VoidSale, RequestRefund, ApproveRefund, RejectRefund, PostRefund, CancelRefund …

**Invariants:**
- Refund is bounded by original settlement; refund state machine is append-only.


**Data owned:** `pos_* + `pos_refund``

**Depends on / depended-by:** depends on `organization`/`identity_access` for scope; collaborators link via `ReferenceQueryService` + Typed Mentions (no direct table joins).

**Referenceable:** yes — `@` + ctx + `/<id>` → `/reference/item/:id` (typed mention). ID example: `@pos/example-28-01-2026-0001`.



## Social surface (paradigm: `docs/plan/04-social-paradigm.md`)

- **Post template:** `SALE-… · {register} · {total} · {status}`.
- **Hashtags:** `#sale #refund #shift` (branch scope).
- **Follow:** sales + refunds; **Lane:** refund approvals for `store-manager` (bounded by settlement).

**Verification:** `cargo xtask test-affected` (owns `crates/contexts/pos`), `cargo xtask verify` for full gate. Traceability: `docs/plan/traceability.md`.

ASCII: see `docs/plan/03-cross-cutting/@-and-ai/pos.md`.
