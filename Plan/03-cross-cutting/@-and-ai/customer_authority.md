# Customer Authority — Deep `@` + AI (Production)

**Group:** `02-customer-support` · **Code:** `crates/contexts/customer_authority` · **Proto:** `crates/proto/proto/erpx/customer_authority/service.proto` (`CustomerAuthorityCommandService, CustomerAuthorityQueryService` · 9 RPCs) · **Adapters:** `crates/adapters/customer_authority-sqlx` · **UI:** `apps/console/src/features/customer_authority` / `crates/ui-domains/*` · **Migrations:** `crates/migration/migrations/000*_*.sql`

**Scope:** Extends `Plan/02-capabilities/02-customer-support/customer_authority.md`. Central polymorphic `@` contract in `Plan/01-vision-and-scope.md` applies verbatim — see `_shared-contract.md` for @ tokenizer, AI lifecycle, Forms, Verify, Spine, ASCII frame.

## Services & RPCs (9)

- **Services:** CustomerAuthorityCommandService, CustomerAuthorityQueryService
- **RPCs:** ** Create, UpdateIdentity, ChangeStatus, LinkAuthority, UnlinkAuthority, Merge, Get, List, GetCustomerAccessAudit

| RPC | Service | Data touch |
|-----|---------|------------|
| `** Create` | Command/Query | `customer_authority` aggregate + outbox |
| `UpdateIdentity` | Command/Query | `customer_authority` aggregate + outbox |
| `ChangeStatus` | Command/Query | `customer_authority` aggregate + outbox |
| `LinkAuthority` | Command/Query | `customer_authority` aggregate + outbox |
| `UnlinkAuthority` | Command/Query | `customer_authority` aggregate + outbox |
| `Merge` | Command/Query | `customer_authority` aggregate + outbox |
| `Get` | Command/Query | `customer_authority` projection |
| `List` | Command/Query | `customer_authority` projection |
| `GetCustomerAccessAudit` | Command/Query | `customer_authority` projection |

## Invariants (domain — keep)

- Customer authority is distinct from identity; bindings are explicit.

Generic 3 (holding-isolation / time-bounded / append-only) — see `_shared-contract.md`.

## Data owned (delta)

- Write: `customer_authority` aggregate + `outbox_events` (CQRS, ADR-002) in holding/branch pair
- Read: rebuildable projection for search; `customer_scope_bindings + object_access_bindings` (light `customer_authority.md:16` canonical)
- Reference graph: see `_shared-contract.md` `record_reference_edges` definition

## Editable fields & schema binding

- **This module:** ** no owner-published schema — OwnerUnsupported (spec-ahead candidate)
- Binding: see `_shared-contract.md` (`MAX_FIELD_PATCH_OPERATIONS=64`, `StableFieldRef`, etc.)

## Business flows (unique)

1. **Happy:** Verify customer identity + authority check — creates typed edges (e.g., `@sales/…`, `@finance/…`) and is queryable via `search`/`backlinks`.
2. **Exception:** see original deep — validates invariants + degraded `Stale/Degraded`.
3. **AI-assisted:** see `_shared-contract.md` (pins ≥3 refs → ghost → Apply/Conflicted).

## Route & UI delta

- **Route:** `/work (authority) + /identity/*` + `/work + /reference/item/:id`
- Shared frame: see `_shared-contract.md` ASCII (title CUSTOMER AUTHORITY, list customer_authority-001/002)

## Traceability

- **Proto:** `crates/proto/proto/erpx/customer_authority/service.proto` + `reference/service.proto`
- **Domain:** `crates/contexts/customer_authority/src/domain.rs`
- **Adapters:** `customer_authority-sqlx`
- **UI:** `apps/console/src/features/customer_authority`
- **Verify:** see `_shared-contract.md` (Q15=C E2E + `cargo xtask test-affected` for `customer_authority`)

## Revision

- Code SHA: fill at PR time (Q18=A). See `Plan/MAP.md` for flat index.
