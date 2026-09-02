# Manufacturing — Deep `@` + AI (Production)

**Group:** `07-manufacturing` · **Code:** `crates/contexts/manufacturing` · **Proto:** `crates/proto/proto/erpx/manufacturing/service.proto` (`ManufacturingCommandService, ManufacturingQueryService` · 18 RPCs) · **Adapters:** `crates/adapters/manufacturing-sqlx` · **UI:** `apps/console/src/features/manufacturing` / `crates/ui-domains/*` · **Migrations:** `crates/migration/migrations/000*_*.sql`

**Scope:** Extends `docs/plan/02-capabilities/07-manufacturing/manufacturing.md`. Central polymorphic `@` contract in `docs/plan/01-vision-and-scope.md` applies verbatim — see `_shared-contract.md` for @ tokenizer, AI lifecycle, Forms, Verify, Spine, ASCII frame.

## Services & RPCs (18)

- **Services:** ManufacturingCommandService, ManufacturingQueryService
- **RPCs:** ** PublishSpecification, CreateOrder, ReleaseOrder, ReserveMaterial, StartOrder, IssueMaterial, ReportLabor, ReportResult, ApplyQualityDisposition, RecordCost, CompleteOrder, CloseOrder, GetSpecification, GetOrder, ListOrders, GetGenealogy, GetWipCost, GetRecord

| RPC | Service | Data touch |
|-----|---------|------------|
| `** PublishSpecification` | Command/Query | `manufacturing` aggregate + outbox |
| `CreateOrder` | Command/Query | `manufacturing` aggregate + outbox |
| `ReleaseOrder` | Command/Query | `manufacturing` aggregate + outbox |
| `ReserveMaterial` | Command/Query | `manufacturing` aggregate + outbox |
| `StartOrder` | Command/Query | `manufacturing` aggregate + outbox |
| `IssueMaterial` | Command/Query | `manufacturing` aggregate + outbox |
| `ReportLabor` | Command/Query | `manufacturing` aggregate + outbox |
| `ReportResult` | Command/Query | `manufacturing` aggregate + outbox |
| `ApplyQualityDisposition` | Command/Query | `manufacturing` aggregate + outbox |
| `RecordCost` | Command/Query | `manufacturing` aggregate + outbox |
| `CompleteOrder` | Command/Query | `manufacturing` aggregate + outbox |
| `CloseOrder` | Command/Query | `manufacturing` aggregate + outbox |

## Invariants (domain — keep)

- Manufacturing orders consume planning + inventory via contracts.

Generic 3 (holding-isolation / time-bounded / append-only) — see `_shared-contract.md`.

## Data owned (delta)

- Write: `manufacturing` aggregate + `outbox_events` (CQRS, ADR-002) in holding/branch pair
- Read: rebuildable projection for search; `manufacturing orders` (light `manufacturing.md:16` canonical)
- Reference graph: see `_shared-contract.md` `record_reference_edges` definition

## Editable fields & schema binding

- **This module:** ** no owner-published schema — OwnerUnsupported (spec-ahead candidate)
- Binding: see `_shared-contract.md` (`MAX_FIELD_PATCH_OPERATIONS=64`, `StableFieldRef`, etc.)

## Business flows (unique)

1. **Happy:** PublishSpec → CreateOrder → Release → Reserve/Issue → ReportLabor/Result → Complete/Close — creates typed edges (e.g., `@sales/…`, `@finance/…`) and is queryable via `search`/`backlinks`.
2. **Exception:** see original deep — validates invariants + degraded `Stale/Degraded`.
3. **AI-assisted:** see `_shared-contract.md` (pins ≥3 refs → ghost → Apply/Conflicted).

## Route & UI delta

- **Route:** `/manufacturing + /operations` + `/work + /reference/item/:id`
- Shared frame: see `_shared-contract.md` ASCII (title MANUFACTURING, list manufacturing-001/002)

## Traceability

- **Proto:** `crates/proto/proto/erpx/manufacturing/service.proto` + `reference/service.proto`
- **Domain:** `crates/contexts/manufacturing/src/domain.rs`
- **Adapters:** `manufacturing-sqlx`
- **UI:** `apps/console/src/features/manufacturing`
- **Verify:** see `_shared-contract.md` (Q15=C E2E + `cargo xtask test-affected` for `manufacturing`)

## Revision

- Code SHA: fill at PR time (Q18=A). See `docs/plan/MAP.md` for flat index.
