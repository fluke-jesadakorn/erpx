# Field Service — Deep `@` + AI (Production)

**Group:** `12-field-service` · **Code:** `crates/contexts/field_service` · **Proto:** `crates/proto/proto/erpx/field_service/service.proto` (`FieldServiceCommandService, FieldServiceQueryService` · 12 RPCs) · **Adapters:** `crates/adapters/field_service-sqlx` · **UI:** `apps/console/src/features/field_service` / `crates/ui-domains/*` · **Migrations:** `crates/migration/migrations/000*_*.sql`

**Scope:** Extends `Plan/02-capabilities/12-field-service/field_service.md`. Central polymorphic `@` contract in `Plan/01-vision-and-scope.md` applies verbatim — see `_shared-contract.md` for @ tokenizer, AI lifecycle, Forms, Verify, Spine, ASCII frame.

## Services & RPCs (12)

- **Services:** FieldServiceCommandService, FieldServiceQueryService
- **RPCs:** ** ActivateAgreement, CreateOrder, ScheduleOrder, ReservePart, DispatchOrder, StartVisit, RecordException, IssuePart, CompleteVisit, ReleaseBilling, GetOrder, GetRecord

| RPC | Service | Data touch |
|-----|---------|------------|
| `** ActivateAgreement` | Command/Query | `field_service` aggregate + outbox |
| `CreateOrder` | Command/Query | `field_service` aggregate + outbox |
| `ScheduleOrder` | Command/Query | `field_service` aggregate + outbox |
| `ReservePart` | Command/Query | `field_service` aggregate + outbox |
| `DispatchOrder` | Command/Query | `field_service` aggregate + outbox |
| `StartVisit` | Command/Query | `field_service` aggregate + outbox |
| `RecordException` | Command/Query | `field_service` aggregate + outbox |
| `IssuePart` | Command/Query | `field_service` aggregate + outbox |
| `CompleteVisit` | Command/Query | `field_service` aggregate + outbox |
| `ReleaseBilling` | Command/Query | `field_service` aggregate + outbox |
| `GetOrder` | Command/Query | `field_service` projection |
| `GetRecord` | Command/Query | `field_service` projection |

## Invariants (domain — keep)

- Service orders require human decision evidence for activation.

Generic 3 (holding-isolation / time-bounded / append-only) — see `_shared-contract.md`.

## Data owned (delta)

- Write: `field_service` aggregate + `outbox_events` (CQRS, ADR-002) in holding/branch pair
- Read: rebuildable projection for search; `field_service orders + agreements` (light `field_service.md:16` canonical)
- Reference graph: see `_shared-contract.md` `record_reference_edges` definition

## Editable fields & schema binding

- **This module:** ** no owner-published schema — OwnerUnsupported (spec-ahead candidate)
- Binding: see `_shared-contract.md` (`MAX_FIELD_PATCH_OPERATIONS=64`, `StableFieldRef`, etc.)

## Business flows (unique)

1. **Happy:** Create/Assign work → Enroute → Work → Close with parts/time — creates typed edges (e.g., `@sales/…`, `@finance/…`) and is queryable via `search`/`backlinks`.
2. **Exception:** see original deep — validates invariants + degraded `Stale/Degraded`.
3. **AI-assisted:** see `_shared-contract.md` (pins ≥3 refs → ghost → Apply/Conflicted).

## Route & UI delta

- **Route:** `/field-service + /maintenance` + `/work + /reference/item/:id`
- Shared frame: see `_shared-contract.md` ASCII (title FIELD SERVICE, list field_service-001/002)

## Traceability

- **Proto:** `crates/proto/proto/erpx/field_service/service.proto` + `reference/service.proto`
- **Domain:** `crates/contexts/field_service/src/domain.rs`
- **Adapters:** `field_service-sqlx`
- **UI:** `apps/console/src/features/field_service`
- **Verify:** see `_shared-contract.md` (Q15=C E2E + `cargo xtask test-affected` for `field_service`)

## Revision

- Code SHA: fill at PR time (Q18=A). See `Plan/MAP.md` for flat index.
