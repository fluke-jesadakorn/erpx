# Crm — Deep `@` + AI (Production)

**Group:** `06-marketing-crm` · **Code:** `crates/contexts/crm` · **Proto:** `crates/proto/proto/erpx/crm/service.proto` (`CrmCommandService, CrmQueryService` · 10 RPCs) · **Adapters:** `crates/adapters/crm-sqlx` · **UI:** `apps/console/src/features/crm` / `crates/ui-domains/*` · **Migrations:** `crates/migration/migrations/000*_*.sql`

**Scope:** Extends `docs/plan/02-capabilities/06-marketing-crm/crm.md`. Central polymorphic `@` contract in `docs/plan/01-vision-and-scope.md` applies verbatim — see `_shared-contract.md` for @ tokenizer, AI lifecycle, Forms, Verify, Spine, ASCII frame.

## Services & RPCs (10)

- **Services:** CrmCommandService, CrmQueryService
- **RPCs:** ** SaveCrmHandoff, SaveCrmOpportunity, RecordCrmAttribution, AcceptMarketingLead, CreateOpportunity, TransitionOpportunity, ListCrmReadRows, GetCrmLead, GetOpportunity, ListOpportunities

| RPC | Service | Data touch |
|-----|---------|------------|
| `** SaveCrmHandoff` | Command/Query | `crm` aggregate + outbox |
| `SaveCrmOpportunity` | Command/Query | `crm` aggregate + outbox |
| `RecordCrmAttribution` | Command/Query | `crm` aggregate + outbox |
| `AcceptMarketingLead` | Command/Query | `crm` aggregate + outbox |
| `CreateOpportunity` | Command/Query | `crm` aggregate + outbox |
| `TransitionOpportunity` | Command/Query | `crm` aggregate + outbox |
| `ListCrmReadRows` | Command/Query | `crm` projection |
| `GetCrmLead` | Command/Query | `crm` projection |
| `GetOpportunity` | Command/Query | `crm` projection |
| `ListOpportunities` | Command/Query | `crm` projection |

## Invariants (domain — keep)

- CRM owns opportunity/account lifecycle independent of sales ordering.

Generic 3 (holding-isolation / time-bounded / append-only) — see `_shared-contract.md`.

## Data owned (delta)

- Write: `crm` aggregate + `outbox_events` (CQRS, ADR-002) in holding/branch pair
- Read: rebuildable projection for search; `crm_*` (light `crm.md:16` canonical)
- Reference graph: see `_shared-contract.md` `record_reference_edges` definition

## Editable fields & schema binding

- **This module:** ** no owner-published schema — OwnerUnsupported (spec-ahead candidate)
- Binding: see `_shared-contract.md` (`MAX_FIELD_PATCH_OPERATIONS=64`, `StableFieldRef`, etc.)

## Business flows (unique)

1. **Happy:** Lead → Opportunity → Activity — creates typed edges (e.g., `@sales/…`, `@finance/…`) and is queryable via `search`/`backlinks`.
2. **Exception:** see original deep — validates invariants + degraded `Stale/Degraded`.
3. **AI-assisted:** see `_shared-contract.md` (pins ≥3 refs → ghost → Apply/Conflicted).

## Feed mapping (paradigm: `docs/plan/04-social-paradigm.md`)

- **Emit:** `CreateOpportunity`→post (revenue story start); stage moves→post updates; handoffs→mention-rich posts.
- **Comments:** open; attribution shown as backlink chips.

## Route & UI delta

- **Route:** `/sales + /marketing (CRM)` + `/work + /reference/item/:id`
- Shared frame: see `_shared-contract.md` ASCII (title CRM, list crm-001/002)

## Traceability

- **Proto:** `crates/proto/proto/erpx/crm/service.proto` + `reference/service.proto`
- **Domain:** `crates/contexts/crm/src/domain.rs`
- **Adapters:** `crm-sqlx`
- **UI:** `apps/console/src/features/crm`
- **Verify:** see `_shared-contract.md` (Q15=C E2E + `cargo xtask test-affected` for `crm`)

## Revision

- Code SHA: fill at PR time (Q18=A). See `docs/plan/MAP.md` for flat index.
