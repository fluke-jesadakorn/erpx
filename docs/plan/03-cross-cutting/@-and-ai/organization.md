# Organization — Deep `@` + AI (Production)

**Group:** `01-platform-governance` · **Code:** `crates/contexts/organization` · **Proto:** `crates/proto/proto/erpx/organization/service.proto` (`OrganizationCommandService, OrganizationQueryService` · 9 RPCs) · **Adapters:** `crates/adapters/organization-sqlx` · **UI:** `apps/console/src/features/organization` / `crates/ui-domains/*` · **Migrations:** `crates/migration/migrations/000*_*.sql`

**Scope:** Extends `docs/plan/02-capabilities/01-platform-governance/organization.md`. Central polymorphic `@` contract in `docs/plan/01-vision-and-scope.md` applies verbatim — see `_shared-contract.md` for @ tokenizer, AI lifecycle, Forms, Verify, Spine, ASCII frame.

## Services & RPCs (9)

- **Services:** OrganizationCommandService, OrganizationQueryService
- **RPCs:** ** CreateNode, RenameNode, ChangeNodeCode, ReparentNode, SuspendNode, ActivateNode, ArchiveNode, GetNode, ListNodes

| RPC | Service | Data touch |
|-----|---------|------------|
| `** CreateNode` | Command/Query | `organization` aggregate + outbox |
| `RenameNode` | Command/Query | `organization` aggregate + outbox |
| `ChangeNodeCode` | Command/Query | `organization` aggregate + outbox |
| `ReparentNode` | Command/Query | `organization` aggregate + outbox |
| `SuspendNode` | Command/Query | `organization` aggregate + outbox |
| `ActivateNode` | Command/Query | `organization` aggregate + outbox |
| `ArchiveNode` | Command/Query | `organization` aggregate + outbox |
| `GetNode` | Command/Query | `organization` projection |
| `ListNodes` | Command/Query | `organization` projection |

## Invariants (domain — keep)

- Holdings/companies/branches are the isolation spine (ADR-001/013); sharing is explicit.

Generic 3 (holding-isolation / time-bounded / append-only) — see `_shared-contract.md`.

## Data owned (delta)

- Write: `organization` aggregate + `outbox_events` (CQRS, ADR-002) in holding/branch pair
- Read: rebuildable projection for search; `holding_registry + organization nodes + customer_holding_bindings` (light `organization.md:16` canonical)
- Reference graph: see `_shared-contract.md` `record_reference_edges` definition

## Editable fields & schema binding

- **This module:** ** no owner-published schema — OwnerUnsupported (spec-ahead candidate)
- Binding: see `_shared-contract.md` (`MAX_FIELD_PATCH_OPERATIONS=64`, `StableFieldRef`, etc.)

## Business flows (unique)

1. **Happy:** Create holding/company/branch node → Activate with infra profile — creates typed edges (e.g., `@sales/…`, `@finance/…`) and is queryable via `search`/`backlinks`.
2. **Exception:** see original deep — validates invariants + degraded `Stale/Degraded`.
3. **AI-assisted:** see `_shared-contract.md` (pins ≥3 refs → ghost → Apply/Conflicted).

## Feed mapping (paradigm: `docs/plan/04-social-paradigm.md`)

- **Emit:** node create/rename/reparent→lineage-audience posts; suspend/archive→lane (`platform`).
- **Comments:** open within lineage; sibling branches see only shared nodes.

## Route & UI delta

- **Route:** `/platform/holdings (PlatformHoldings) + /operations` + `/work + /reference/item/:id`
- Shared frame: see `_shared-contract.md` ASCII (title ORGANIZATION, list organization-001/002)

## Traceability

- **Proto:** `crates/proto/proto/erpx/organization/service.proto` + `reference/service.proto`
- **Domain:** `crates/contexts/organization/src/domain.rs`
- **Adapters:** `organization-sqlx`
- **UI:** `apps/console/src/features/organization`
- **Verify:** see `_shared-contract.md` (Q15=C E2E + `cargo xtask test-affected` for `organization`)

## Revision

- Code SHA: fill at PR time (Q18=A). See `docs/plan/MAP.md` for flat index.
