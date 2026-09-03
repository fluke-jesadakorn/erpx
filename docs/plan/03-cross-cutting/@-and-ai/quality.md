# Quality — Deep `@` + AI (Production)

**Group:** `04-supply-chain-ops` · **Code:** `crates/contexts/quality` · **Proto:** `crates/proto/proto/erpx/quality/service.proto` (`QualityCommandService, QualityQueryService` · 7 RPCs) · **Adapters:** `crates/adapters/quality-sqlx` · **UI:** `apps/console/src/features/quality` / `crates/ui-domains/*` · **Migrations:** `crates/migration/migrations/000*_*.sql`

**Scope:** Extends `docs/plan/02-capabilities/04-supply-chain-ops/quality.md`. Central polymorphic `@` contract in `docs/plan/01-vision-and-scope.md` applies verbatim — see `_shared-contract.md` for @ tokenizer, AI lifecycle, Forms, Verify, Spine, ASCII frame.

## Services & RPCs (7)

- **Services:** QualityCommandService, QualityQueryService
- **RPCs:** ** Create, Start, RecordResult, Release, Cancel, Get, List

| RPC | Service | Data touch |
|-----|---------|------------|
| `** Create` | Command/Query | `quality` aggregate + outbox |
| `Start` | Command/Query | `quality` aggregate + outbox |
| `RecordResult` | Command/Query | `quality` aggregate + outbox |
| `Release` | Command/Query | `quality` aggregate + outbox |
| `Cancel` | Command/Query | `quality` aggregate + outbox |
| `Get` | Command/Query | `quality` projection |
| `List` | Command/Query | `quality` projection |

## Invariants (domain — keep)

- Quality checks gate operations completion.

Generic 3 (holding-isolation / time-bounded / append-only) — see `_shared-contract.md`.

## Data owned (delta)

- Write: `quality` aggregate + `outbox_events` (CQRS, ADR-002) in holding/branch pair
- Read: rebuildable projection for search; `quality_*` (light `quality.md:16` canonical)
- Reference graph: see `_shared-contract.md` `record_reference_edges` definition

## Editable fields & schema binding

- **This module:** ** no owner-published schema — OwnerUnsupported (spec-ahead candidate)
- Binding: see `_shared-contract.md` (`MAX_FIELD_PATCH_OPERATIONS=64`, `StableFieldRef`, etc.)

## Business flows (unique)

1. **Happy:** Create inspection lot → RecordResult → Release/Cancel — creates typed edges (e.g., `@sales/…`, `@finance/…`) and is queryable via `search`/`backlinks`.
2. **Exception:** see original deep — validates invariants + degraded `Stale/Degraded`.
3. **AI-assisted:** see `_shared-contract.md` (pins ≥3 refs → ghost → Apply/Conflicted).

## Feed mapping (paradigm: `docs/plan/04-social-paradigm.md`)

- **Emit:** `Create`→post; failed `RecordResult`→lane (`quality-lead`) + blocks ops visibly; `Release`→post update.
- **Comments:** open; gate state on the card.

## Route & UI delta

- **Route:** `/quality + /operations` + `/work + /reference/item/:id`
- Shared frame: see `_shared-contract.md` ASCII (title QUALITY, list quality-001/002)

## Traceability

- **Proto:** `crates/proto/proto/erpx/quality/service.proto` + `reference/service.proto`
- **Domain:** `crates/contexts/quality/src/domain.rs`
- **Adapters:** `quality-sqlx`
- **UI:** `apps/console/src/features/quality`
- **Verify:** see `_shared-contract.md` (Q15=C E2E + `cargo xtask test-affected` for `quality`)

## Revision

- Code SHA: fill at PR time (Q18=A). See `docs/plan/MAP.md` for flat index.
