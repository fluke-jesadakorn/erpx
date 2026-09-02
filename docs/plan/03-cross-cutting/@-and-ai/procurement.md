# Procurement — Deep `@` + AI (Production)

**Group:** `04-supply-chain-ops` · **Code:** `crates/contexts/procurement` · **Proto:** `crates/proto/proto/erpx/procurement/service.proto` (`ProcurementCommandService, ProcurementQueryService` · 16 RPCs) · **Adapters:** `crates/adapters/procurement-sqlx` · **UI:** `apps/console/src/features/procurement` / `crates/ui-domains/*` · **Migrations:** `crates/migration/migrations/000*_*.sql`

**Scope:** Extends `docs/plan/02-capabilities/04-supply-chain-ops/procurement.md`. Central polymorphic `@` contract in `docs/plan/01-vision-and-scope.md` applies verbatim — see `_shared-contract.md` for @ tokenizer, AI lifecycle, Forms, Verify, Spine, ASCII frame.

## Services & RPCs (16)

- **Services:** ProcurementCommandService, ProcurementQueryService
- **RPCs:** ** Create, Release, Cancel, CreateRequisition, SubmitRequisition, ApproveRequisition, RejectRequisition, CancelRequisition, CreateGoodsReceipt, AcceptGoodsReceipt, RejectGoodsReceipt, CancelGoodsReceipt, Get, List, GetRequisition, GetGoodsReceipt

| RPC | Service | Data touch |
|-----|---------|------------|
| `** Create` | Command/Query | `procurement` aggregate + outbox |
| `Release` | Command/Query | `procurement` aggregate + outbox |
| `Cancel` | Command/Query | `procurement` aggregate + outbox |
| `CreateRequisition` | Command/Query | `procurement` aggregate + outbox |
| `SubmitRequisition` | Command/Query | `procurement` aggregate + outbox |
| `ApproveRequisition` | Command/Query | `procurement` aggregate + outbox |
| `RejectRequisition` | Command/Query | `procurement` aggregate + outbox |
| `CancelRequisition` | Command/Query | `procurement` aggregate + outbox |
| `CreateGoodsReceipt` | Command/Query | `procurement` aggregate + outbox |
| `AcceptGoodsReceipt` | Command/Query | `procurement` aggregate + outbox |
| `RejectGoodsReceipt` | Command/Query | `procurement` aggregate + outbox |
| `CancelGoodsReceipt` | Command/Query | `procurement` aggregate + outbox |

## Invariants (domain — keep)

- Procurement references inventory/operations via typed contracts, not shared tables.

Generic 3 (holding-isolation / time-bounded / append-only) — see `_shared-contract.md`.

## Data owned (delta)

- Write: `procurement` aggregate + `outbox_events` (CQRS, ADR-002) in holding/branch pair
- Read: rebuildable projection for search; `procurement_*` (light `procurement.md:16` canonical)
- Reference graph: see `_shared-contract.md` `record_reference_edges` definition

## Editable fields & schema binding

- **This module:** ** procurement.requisition: REQUISITION_FIELDS (requisition.justification Text)
- Binding: see `_shared-contract.md` (`MAX_FIELD_PATCH_OPERATIONS=64`, `StableFieldRef`, etc.)

## Business flows (unique)

1. **Happy:** Requisition → Approve → PO → Receipt → Invoice match — creates typed edges (e.g., `@sales/…`, `@finance/…`) and is queryable via `search`/`backlinks`.
2. **Exception:** see original deep — validates invariants + degraded `Stale/Degraded`.
3. **AI-assisted:** see `_shared-contract.md` (pins ≥3 refs → ghost → Apply/Conflicted).

## Route & UI delta

- **Route:** `/procurement + /work + /reference/item/:id` + `/work + /reference/item/:id`
- Shared frame: see `_shared-contract.md` ASCII (title PROCUREMENT, list procurement-001/002)

## Traceability

- **Proto:** `crates/proto/proto/erpx/procurement/service.proto` + `reference/service.proto`
- **Domain:** `crates/contexts/procurement/src/domain.rs`
- **Adapters:** `procurement-sqlx`
- **UI:** `apps/console/src/features/procurement`
- **Verify:** see `_shared-contract.md` (Q15=C E2E + `cargo xtask test-affected` for `procurement`)

## Revision

- Code SHA: fill at PR time (Q18=A). See `docs/plan/MAP.md` for flat index.
