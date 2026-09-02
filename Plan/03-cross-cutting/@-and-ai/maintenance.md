# Maintenance — Deep `@` + AI (Production)

**Group:** `04-supply-chain-ops` · **Code:** `crates/contexts/maintenance` · **Proto:** `crates/proto/proto/erpx/maintenance/service.proto` (`MaintenanceCommandService, MaintenanceQueryService` · 10 RPCs) · **Adapters:** `crates/adapters/maintenance-sqlx` · **UI:** `apps/console/src/features/maintenance` / `crates/ui-domains/*` · **Migrations:** `crates/migration/migrations/000*_*.sql`

**Scope:** Extends `Plan/02-capabilities/04-supply-chain-ops/maintenance.md`. Central polymorphic `@` contract in `Plan/01-vision-and-scope.md` applies verbatim — see `_shared-contract.md` for @ tokenizer, AI lifecycle, Forms, Verify, Spine, ASCII frame.

## Services & RPCs (10)

- **Services:** MaintenanceCommandService, MaintenanceQueryService
- **RPCs:** ** Open, Schedule, Start, Block, Resume, Complete, Cancel, RecordDowntime, Get, List

| RPC | Service | Data touch |
|-----|---------|------------|
| `** Open` | Command/Query | `maintenance` aggregate + outbox |
| `Schedule` | Command/Query | `maintenance` aggregate + outbox |
| `Start` | Command/Query | `maintenance` aggregate + outbox |
| `Block` | Command/Query | `maintenance` aggregate + outbox |
| `Resume` | Command/Query | `maintenance` aggregate + outbox |
| `Complete` | Command/Query | `maintenance` aggregate + outbox |
| `Cancel` | Command/Query | `maintenance` aggregate + outbox |
| `RecordDowntime` | Command/Query | `maintenance` aggregate + outbox |
| `Get` | Command/Query | `maintenance` projection |
| `List` | Command/Query | `maintenance` projection |

## Invariants (domain — keep)

- Work orders bound to assets; completion requires evidence.

Generic 3 (holding-isolation / time-bounded / append-only) — see `_shared-contract.md`.

## Data owned (delta)

- Write: `maintenance` aggregate + `outbox_events` (CQRS, ADR-002) in holding/branch pair
- Read: rebuildable projection for search; `maintenance work orders` (light `maintenance.md:16` canonical)
- Reference graph: see `_shared-contract.md` `record_reference_edges` definition

## Editable fields & schema binding

- **This module:** ** maintenance.request: MAINTENANCE_REQUEST_FIELDS (maintenance_request.description Text)
- Binding: see `_shared-contract.md` (`MAX_FIELD_PATCH_OPERATIONS=64`, `StableFieldRef`, etc.)

## Business flows (unique)

1. **Happy:** Open WO → Schedule → Start → Block/Resume → Complete + downtime — creates typed edges (e.g., `@sales/…`, `@finance/…`) and is queryable via `search`/`backlinks`.
2. **Exception:** see original deep — validates invariants + degraded `Stale/Degraded`.
3. **AI-assisted:** see `_shared-contract.md` (pins ≥3 refs → ghost → Apply/Conflicted).

## Route & UI delta

- **Route:** `/maintenance + /field-service` + `/work + /reference/item/:id`
- Shared frame: see `_shared-contract.md` ASCII (title MAINTENANCE, list maintenance-001/002)

## Traceability

- **Proto:** `crates/proto/proto/erpx/maintenance/service.proto` + `reference/service.proto`
- **Domain:** `crates/contexts/maintenance/src/domain.rs`
- **Adapters:** `maintenance-sqlx`
- **UI:** `apps/console/src/features/maintenance`
- **Verify:** see `_shared-contract.md` (Q15=C E2E + `cargo xtask test-affected` for `maintenance`)

## Revision

- Code SHA: fill at PR time (Q18=A). See `Plan/MAP.md` for flat index.
