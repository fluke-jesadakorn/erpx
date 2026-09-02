# Customer 360 — Deep `@` + AI (Production)

**Group:** `02-customer-support` · **Code:** `crates/contexts/customer_360` · **Proto:** `crates/proto/proto/erpx/customer_360/service.proto` (`Customer360CommandService, Customer360QueryService` · 3 RPCs) · **Adapters:** `crates/adapters/customer_360-sqlx` · **UI:** `apps/console/src/features/customer_360` / `crates/ui-domains/*` · **Migrations:** `crates/migration/migrations/000*_*.sql`

**Scope:** Extends `docs/plan/02-capabilities/02-customer-support/customer_360.md`. Central polymorphic `@` contract in `docs/plan/01-vision-and-scope.md` applies verbatim — see `_shared-contract.md` for @ tokenizer, AI lifecycle, Forms, Verify, Spine, ASCII frame.

## Services & RPCs (3)

- **Services:** Customer360CommandService, Customer360QueryService
- **RPCs:** ** AddCustomerNote, GetCustomer360, GetCustomer360Detail

| RPC | Service | Data touch |
|-----|---------|------------|
| `** AddCustomerNote` | Command/Query | `customer_360` aggregate + outbox |
| `GetCustomer360` | Command/Query | `customer_360` projection |
| `GetCustomer360Detail` | Command/Query | `customer_360` projection |

## Invariants (domain — keep)

- Customer 360 is a federated read projection, not a write owner.

Generic 3 (holding-isolation / time-bounded / append-only) — see `_shared-contract.md`.

## Data owned (delta)

- Write: `customer_360` aggregate + `outbox_events` (CQRS, ADR-002) in holding/branch pair
- Read: rebuildable projection for search; `federated read (no owned tables)` (light `customer_360.md:16` canonical)
- Reference graph: see `_shared-contract.md` `record_reference_edges` definition

## Editable fields & schema binding

- **This module:** ** customer-360.note: CUSTOMER_NOTE_FIELDS (customer_note.body Text)
- Binding: see `_shared-contract.md` (`MAX_FIELD_PATCH_OPERATIONS=64`, `StableFieldRef`, etc.)

## Business flows (unique)

1. **Happy:** Create customer note + timeline event — creates typed edges (e.g., `@sales/…`, `@finance/…`) and is queryable via `search`/`backlinks`.
2. **Exception:** see original deep — validates invariants + degraded `Stale/Degraded`.
3. **AI-assisted:** see `_shared-contract.md` (pins ≥3 refs → ghost → Apply/Conflicted).

## Route & UI delta

- **Route:** `/work (Customer 360) + /reference/item/:id` + `/work + /reference/item/:id`
- Shared frame: see `_shared-contract.md` ASCII (title CUSTOMER 360, list customer_360-001/002)

## Traceability

- **Proto:** `crates/proto/proto/erpx/customer_360/service.proto` + `reference/service.proto`
- **Domain:** `crates/contexts/customer_360/src/domain.rs`
- **Adapters:** `customer_360-sqlx`
- **UI:** `apps/console/src/features/customer_360`
- **Verify:** see `_shared-contract.md` (Q15=C E2E + `cargo xtask test-affected` for `customer_360`)

## Revision

- Code SHA: fill at PR time (Q18=A). See `docs/plan/MAP.md` for flat index.
