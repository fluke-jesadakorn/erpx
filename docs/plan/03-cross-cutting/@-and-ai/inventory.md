# Inventory — Deep `@` + AI (Production)

**Group:** `04-supply-chain-ops` · **Code:** `crates/contexts/inventory` · **Proto:** `crates/proto/proto/erpx/inventory/service.proto` (`InventoryCommandService, InventoryQueryService` · 7 RPCs) · **Adapters:** `crates/adapters/inventory-sqlx` · **UI:** `apps/console/src/features/inventory` / `crates/ui-domains/*` · **Migrations:** `crates/migration/migrations/000*_*.sql`

**Scope:** Extends `docs/plan/02-capabilities/04-supply-chain-ops/inventory.md`. Central polymorphic `@` contract in `docs/plan/01-vision-and-scope.md` applies verbatim — see `_shared-contract.md` for @ tokenizer, AI lifecycle, Forms, Verify, Spine, ASCII frame.

## Services & RPCs (7)

- **Services:** InventoryCommandService, InventoryQueryService
- **RPCs:** ** Register, Receive, Issue, Reserve, ReleaseReservation, Get, List

| RPC | Service | Data touch |
|-----|---------|------------|
| `** Register` | Command/Query | `inventory` aggregate + outbox |
| `Receive` | Command/Query | `inventory` aggregate + outbox |
| `Issue` | Command/Query | `inventory` aggregate + outbox |
| `Reserve` | Command/Query | `inventory` aggregate + outbox |
| `ReleaseReservation` | Command/Query | `inventory` aggregate + outbox |
| `Get` | Command/Query | `inventory` projection |
| `List` | Command/Query | `inventory` projection |

## Invariants (domain — keep)

- Stock moves are transactional with outbox; projections rebuildable from events.

Generic 3 (holding-isolation / time-bounded / append-only) — see `_shared-contract.md`.

## Data owned (delta)

- Write: `inventory` aggregate + `outbox_events` (CQRS, ADR-002) in holding/branch pair
- Read: rebuildable projection for search; `inventory_* + outbox → projection` (light `inventory.md:16` canonical)
- Reference graph: see `_shared-contract.md` `record_reference_edges` definition

## Editable fields & schema binding

- **This module:** ** no owner-published schema — OwnerUnsupported (spec-ahead candidate)
- Binding: see `_shared-contract.md` (`MAX_FIELD_PATCH_OPERATIONS=64`, `StableFieldRef`, etc.)

## Business flows (unique)

1. **Happy:** Receive → Putaway → Move → Count adjustment — creates typed edges (e.g., `@sales/…`, `@finance/…`) and is queryable via `search`/`backlinks`.
2. **Exception:** see original deep — validates invariants + degraded `Stale/Degraded`.
3. **AI-assisted:** see `_shared-contract.md` (pins ≥3 refs → ghost → Apply/Conflicted).

## Route & UI delta

- **Route:** `/inventory + /work` + `/work + /reference/item/:id`
- Shared frame: see `_shared-contract.md` ASCII (title INVENTORY, list inventory-001/002)

## Traceability

- **Proto:** `crates/proto/proto/erpx/inventory/service.proto` + `reference/service.proto`
- **Domain:** `crates/contexts/inventory/src/domain.rs`
- **Adapters:** `inventory-sqlx`
- **UI:** `apps/console/src/features/inventory`
- **Verify:** see `_shared-contract.md` (Q15=C E2E + `cargo xtask test-affected` for `inventory`)

## Revision

- Code SHA: fill at PR time (Q18=A). See `docs/plan/MAP.md` for flat index.
