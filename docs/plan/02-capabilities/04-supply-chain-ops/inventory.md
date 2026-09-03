# Inventory

**Purpose:** Owns stock and inventory projections.

**Group:** `04-supply-chain-ops` · **Code:** `crates/contexts/inventory` · **Proto:** `crates/proto/proto/erpx/inventory/service.proto` · **Adapters:** `crates/adapters/inventory-sqlx` (where present) · **UI:** `apps/console/src/features/inventory` / `crates/ui-domains/*`

**Actors & scopes:** `AuthorityScope::{Platform, Holding, Company, Branch, Subtree}` via `crates/kernel`; RBAC through expiring `role_assignments` (ADR-004) where context is gated.

**Services & RPCs (7):** InventoryCommandService, InventoryQueryService
- Register, Receive, Issue, Reserve, ReleaseReservation, Get, List

**Invariants:**
- Stock moves are transactional with outbox; projections rebuildable from events.


**Data owned:** `inventory_* + outbox → projection`

**Depends on / depended-by:** depends on `organization`/`identity_access` for scope; collaborators link via `ReferenceQueryService` + Typed Mentions (no direct table joins).

**Referenceable:** yes — `@` + ctx + `/<id>` → `/reference/item/:id` (typed mention). ID example: `@inventory/example-28-01-2026-0001`.



## Social surface (paradigm: `docs/plan/04-social-paradigm.md`)

- **Post template:** `MOVE-… · {sku} · {qty} · {from→to}` — moves post on commit.
- **Hashtags:** `#stock #shortage` (branch scope).
- **Follow:** watched SKUs; **Lane:** shortages/blocked reservations for `warehouse` role.

**Verification:** `cargo xtask test-affected` (owns `crates/contexts/inventory`), `cargo xtask verify` for full gate. Traceability: `docs/plan/traceability.md`.

ASCII: see `docs/plan/03-cross-cutting/@-and-ai/inventory.md`.
