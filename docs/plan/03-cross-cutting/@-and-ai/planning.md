# Planning — Deep `@` + AI (Production)

**Group:** `08-planning` · **Code:** `crates/contexts/planning` · **Proto:** `crates/proto/proto/erpx/planning/service.proto` (`PlanningCommandService, PlanningQueryService` · 14 RPCs) · **Adapters:** `crates/adapters/planning-sqlx` · **UI:** `apps/console/src/features/planning` / `crates/ui-domains/*` · **Migrations:** `crates/migration/migrations/000*_*.sql`

**Scope:** Extends `docs/plan/02-capabilities/08-planning/planning.md`. Central polymorphic `@` contract in `docs/plan/01-vision-and-scope.md` applies verbatim — see `_shared-contract.md` for @ tokenizer, AI lifecycle, Forms, Verify, Spine, ASCII frame.

## Services & RPCs (14)

- **Services:** PlanningCommandService, PlanningQueryService
- **RPCs:** ** CreateScenario, FreezeScenario, RunScenario, OverrideRecommendation, ApproveRecommendation, RejectRecommendation, ReleaseRecommendation, GetScenario, GetRecommendation, GetRun, ListRecommendations, ListShortages, ListCapacityOverloads, GetPlan

| RPC | Service | Data touch |
|-----|---------|------------|
| `** CreateScenario` | Command/Query | `planning` aggregate + outbox |
| `FreezeScenario` | Command/Query | `planning` aggregate + outbox |
| `RunScenario` | Command/Query | `planning` aggregate + outbox |
| `OverrideRecommendation` | Command/Query | `planning` aggregate + outbox |
| `ApproveRecommendation` | Command/Query | `planning` aggregate + outbox |
| `RejectRecommendation` | Command/Query | `planning` aggregate + outbox |
| `ReleaseRecommendation` | Command/Query | `planning` aggregate + outbox |
| `GetScenario` | Command/Query | `planning` projection |
| `GetRecommendation` | Command/Query | `planning` projection |
| `GetRun` | Command/Query | `planning` projection |
| `ListRecommendations` | Command/Query | `planning` projection |
| `ListShortages` | Command/Query | `planning` projection |

## Invariants (domain — keep)

- Supply & demand planning is projection-driven, not a write owner of stock.

Generic 3 (holding-isolation / time-bounded / append-only) — see `_shared-contract.md`.

## Data owned (delta)

- Write: `planning` aggregate + `outbox_events` (CQRS, ADR-002) in holding/branch pair
- Read: rebuildable projection for search; `planning projections` (light `planning.md:16` canonical)
- Reference graph: see `_shared-contract.md` `record_reference_edges` definition

## Editable fields & schema binding

- **This module:** ** no owner-published schema — OwnerUnsupported (spec-ahead candidate)
- Binding: see `_shared-contract.md` (`MAX_FIELD_PATCH_OPERATIONS=64`, `StableFieldRef`, etc.)

## Business flows (unique)

1. **Happy:** Demand forecast → Supply plan → Reconcile — creates typed edges (e.g., `@sales/…`, `@finance/…`) and is queryable via `search`/`backlinks`.
2. **Exception:** see original deep — validates invariants + degraded `Stale/Degraded`.
3. **AI-assisted:** see `_shared-contract.md` (pins ≥3 refs → ghost → Apply/Conflicted).

## Route & UI delta

- **Route:** `/planning + /operations` + `/work + /reference/item/:id`
- Shared frame: see `_shared-contract.md` ASCII (title PLANNING, list planning-001/002)

## Traceability

- **Proto:** `crates/proto/proto/erpx/planning/service.proto` + `reference/service.proto`
- **Domain:** `crates/contexts/planning/src/domain.rs`
- **Adapters:** `planning-sqlx`
- **UI:** `apps/console/src/features/planning`
- **Verify:** see `_shared-contract.md` (Q15=C E2E + `cargo xtask test-affected` for `planning`)

## Revision

- Code SHA: fill at PR time (Q18=A). See `docs/plan/MAP.md` for flat index.
