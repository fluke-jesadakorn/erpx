# Assets — Deep `@` + AI (Production)

**Group:** `04-supply-chain-ops` · **Code:** `crates/contexts/assets` · **Proto:** `crates/proto/proto/erpx/assets/service.proto` (`AssetsCommandService, AssetsQueryService` · 8 RPCs) · **Adapters:** `crates/adapters/assets-sqlx` · **UI:** `apps/console/src/features/assets` / `crates/ui-domains/*` · **Migrations:** `crates/migration/migrations/000*_*.sql`

**Scope:** Extends `docs/plan/02-capabilities/04-supply-chain-ops/assets.md`. Central polymorphic `@` contract in `docs/plan/01-vision-and-scope.md` applies verbatim — see `_shared-contract.md` for @ tokenizer, AI lifecycle, Forms, Verify, Spine, ASCII frame.

## Services & RPCs (8)

- **Services:** AssetsCommandService, AssetsQueryService
- **RPCs:** ** Create, PlaceInService, MarkDegraded, BeginMaintenance, ReturnToService, Retire, Get, List

| RPC | Service | Data touch |
|-----|---------|------------|
| `** Create` | Command/Query | `assets` aggregate + outbox |
| `PlaceInService` | Command/Query | `assets` aggregate + outbox |
| `MarkDegraded` | Command/Query | `assets` aggregate + outbox |
| `BeginMaintenance` | Command/Query | `assets` aggregate + outbox |
| `ReturnToService` | Command/Query | `assets` aggregate + outbox |
| `Retire` | Command/Query | `assets` aggregate + outbox |
| `Get` | Command/Query | `assets` projection |
| `List` | Command/Query | `assets` projection |

## Invariants (domain — keep)

- Lifecycle: draft → in-service → degraded/under-maintenance → retired — illegal transitions rejected.

Generic 3 (holding-isolation / time-bounded / append-only) — see `_shared-contract.md`.

## Data owned (delta)

- Write: `assets` aggregate + `outbox_events` (CQRS, ADR-002) in holding/branch pair
- Read: rebuildable projection for search; `assets_* + maintenance link` (light `assets.md:16` canonical)
- Reference graph: see `_shared-contract.md` `record_reference_edges` definition

## Editable fields & schema binding

- **This module:** ** no owner-published schema — OwnerUnsupported (spec-ahead candidate)
- Binding: see `_shared-contract.md` (`MAX_FIELD_PATCH_OPERATIONS=64`, `StableFieldRef`, etc.)

## Business flows (unique)

1. **Happy:** Create asset → PlaceInService → Degraded → Maintenance → ReturnToService → Retire — creates typed edges (e.g., `@sales/…`, `@finance/…`) and is queryable via `search`/`backlinks`.
2. **Exception:** see original deep — validates invariants + degraded `Stale/Degraded`.
3. **AI-assisted:** see `_shared-contract.md` (pins ≥3 refs → ghost → Apply/Conflicted).

## Feed mapping (paradigm: `docs/plan/04-social-paradigm.md`)

- **Emit:** lifecycle transitions→post updates; `MarkDegraded`→lane (`ops`).
- **Comments:** open; maintenance link shown as backlink chip.

## Route & UI delta

- **Route:** `/asset + /operations` + `/work + /reference/item/:id`
- Shared frame: see `_shared-contract.md` ASCII (title ASSETS, list assets-001/002)

## Traceability

- **Proto:** `crates/proto/proto/erpx/assets/service.proto` + `reference/service.proto`
- **Domain:** `crates/contexts/assets/src/domain.rs`
- **Adapters:** `assets-sqlx`
- **UI:** `apps/console/src/features/assets`
- **Verify:** see `_shared-contract.md` (Q15=C E2E + `cargo xtask test-affected` for `assets`)

## Revision

- Code SHA: fill at PR time (Q18=A). See `docs/plan/MAP.md` for flat index.
