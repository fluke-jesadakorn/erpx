# Brand — Deep `@` + AI (Production)

**Group:** `01-platform-governance` · **Code:** `crates/contexts/brand` · **Proto:** `crates/proto/proto/erpx/brand/service.proto` (`BrandCommandService, BrandQueryService` · 8 RPCs) · **Adapters:** `crates/adapters/brand-sqlx` · **UI:** `apps/console/src/features/brand` / `crates/ui-domains/*` · **Migrations:** `crates/migration/migrations/000*_*.sql`

**Scope:** Extends `docs/plan/02-capabilities/01-platform-governance/brand.md`. Central polymorphic `@` contract in `docs/plan/01-vision-and-scope.md` applies verbatim — see `_shared-contract.md` for @ tokenizer, AI lifecycle, Forms, Verify, Spine, ASCII frame.

## Services & RPCs (8)

- **Services:** BrandCommandService, BrandQueryService
- **RPCs:** ** CreateDraft, SaveDraft, PublishDraft, Rollback, SetCategoryLocks, GetDraft, GetResolved, ListHistory

| RPC | Service | Data touch |
|-----|---------|------------|
| `** CreateDraft` | Command/Query | `brand` aggregate + outbox |
| `SaveDraft` | Command/Query | `brand` aggregate + outbox |
| `PublishDraft` | Command/Query | `brand` aggregate + outbox |
| `Rollback` | Command/Query | `brand` aggregate + outbox |
| `SetCategoryLocks` | Command/Query | `brand` aggregate + outbox |
| `GetDraft` | Command/Query | `brand` projection |
| `GetResolved` | Command/Query | `brand` projection |
| `ListHistory` | Command/Query | `brand` projection |

## Invariants (domain — keep)

- Brand drafts are versioned with category locks; publish is atomic.

Generic 3 (holding-isolation / time-bounded / append-only) — see `_shared-contract.md`.

## Data owned (delta)

- Write: `brand` aggregate + `outbox_events` (CQRS, ADR-002) in holding/branch pair
- Read: rebuildable projection for search; `brand_drafts + category locks` (light `brand.md:16` canonical)
- Reference graph: see `_shared-contract.md` `record_reference_edges` definition

## Editable fields & schema binding

- **This module:** ** no owner-published schema — OwnerUnsupported (spec-ahead candidate)
- Binding: see `_shared-contract.md` (`MAX_FIELD_PATCH_OPERATIONS=64`, `StableFieldRef`, etc.)

## Business flows (unique)

1. **Happy:** Draft corporate identity → PublishCorporateIdentityDraft — creates typed edges (e.g., `@sales/…`, `@finance/…`) and is queryable via `search`/`backlinks`.
2. **Exception:** see original deep — validates invariants + degraded `Stale/Degraded`.
3. **AI-assisted:** see `_shared-contract.md` (pins ≥3 refs → ghost → Apply/Conflicted).

## Route & UI delta

- **Route:** `/platform (corporate identity) + /marketing` + `/work + /reference/item/:id`
- Shared frame: see `_shared-contract.md` ASCII (title BRAND, list brand-001/002)

## Traceability

- **Proto:** `crates/proto/proto/erpx/brand/service.proto` + `reference/service.proto`
- **Domain:** `crates/contexts/brand/src/domain.rs`
- **Adapters:** `brand-sqlx`
- **UI:** `apps/console/src/features/brand`
- **Verify:** see `_shared-contract.md` (Q15=C E2E + `cargo xtask test-affected` for `brand`)

## Revision

- Code SHA: fill at PR time (Q18=A). See `docs/plan/MAP.md` for flat index.
