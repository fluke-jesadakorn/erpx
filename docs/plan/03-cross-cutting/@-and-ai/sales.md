# Sales — Deep `@` + AI (Production)

**Group:** `03-finance-sales-pos` · **Code:** `crates/contexts/sales` · **Proto:** `crates/proto/proto/erpx/sales/service.proto` (`SalesCommandService, SalesQueryService` · 13 RPCs) · **Adapters:** `crates/adapters/sales-sqlx` · **UI:** `apps/console/src/features/sales`

**Scope:** Extends `docs/plan/02-capabilities/03-finance-sales-pos/sales.md`. Central polymorphic `@` contract in `docs/plan/01-vision-and-scope.md` applies verbatim — see `_shared-contract.md` for @ tokenizer, AI lifecycle, Forms, Verify, Spine, ASCII frame.

## Services & RPCs (13)

- **Services:** SalesCommandService, SalesQueryService
- **RPCs:** CreateQuote, SendQuote, AcceptQuote, RejectQuote, ExpireQuote, CreateSalesOrder, ConfirmSalesOrder, MarkSalesOrderPartiallyFulfilled, MarkSalesOrderFulfilled, CancelSalesOrder, GetQuote, GetSalesOrder, ListSalesOrders

| RPC | Service | Data touch |
|-----|---------|------------|
| `CreateQuote` | Command | `sales` aggregate + outbox |
| `SendQuote` | Command | `sales` aggregate + outbox |
| `AcceptQuote` | Command | `sales` aggregate + outbox |
| `RejectQuote` | Command | `sales` aggregate + outbox |
| `ExpireQuote` | Command | `sales` aggregate + outbox |
| `CreateSalesOrder` | Command | `sales` aggregate + outbox |
| `ConfirmSalesOrder` | Command | `sales` aggregate + outbox |
| `MarkSalesOrderPartiallyFulfilled` | Command | `sales` aggregate + outbox |
| `MarkSalesOrderFulfilled` | Command | `sales` aggregate + outbox |
| `CancelSalesOrder` | Command | `sales` aggregate + outbox |
| `GetQuote` | Query | `sales` projection |
| `GetSalesOrder` | Query | `sales` projection |
| `ListSalesOrders` | Query | `sales` projection |

## Invariants (domain — keep, not generic)

- Quote → order → posting chain validated in `sales` domain before finance posting.
- No direct finance writes from sales outside posting application.

Generic 3 (holding-isolation / time-bounded / append-only) — see `_shared-contract.md`.

## Data owned (delta)

- Write: `sales` aggregate + `outbox_events` (CQRS, ADR-002) in holding/branch pair
- Read: rebuildable projection for search; `sales projection + finance posting link` (see light `sales.md:16`)
- Reference graph: see `_shared-contract.md` `record_reference_edges` definition

## Editable fields & schema binding

- **This module:** `sales.quote: SALES_QUOTE_FIELDS` (quote.notes Text 2,048 Internal/Low, quote.payment_terms_days Integer 0-365, quote.valid_until DateTime, quote.language Choice en/th) — `OwnerPublished` (only sales + procurement)
- Binding: see `_shared-contract.md` (`MAX_FIELD_PATCH_OPERATIONS=64`, `StableFieldRef`, `previous_value_digest`, etc.)

## Business flows (unique)

1. **Happy:** CreateQuote → Send → Accept → CreateSalesOrder → Fulfill — creates typed edges, queryable via `search`/`backlinks`.
2. **Exception:** Reject/Expire + credit — validates invariants + degraded `Stale/Degraded`.
3. **AI-assisted:** pins ≥3 refs (`@projects/proj-…`, `@procurement/req-…`, `@finance/inv-…`) → ghost preview → Apply only on version match else Conflicted — see `_shared-contract.md` for lifecycle.

## Route & UI delta

- **Route:** `/sales + /work + /reference/item/:id`
- **Spec row:** `quote.notes | Text | 2,048 bytes | sales.quote.suggestion.apply | @-chips inline, ghost-fill from @projects + @crm`
- Shared frame: see `_shared-contract.md` ASCII (title SALES, list sales-001/002/003, chips `@projects/proj-2026-001` etc.)

## Traceability

- **Proto:** `crates/proto/proto/erpx/sales/service.proto` + `reference/service.proto`
- **Domain:** `crates/contexts/sales/src/domain.rs`
- **Contracts:** `editable_fields.rs`, `catalog.rs`, `suggestion.rs`
- **Adapters:** `sales-sqlx`, `migrations 000*`
- **UI:** `apps/console/src/features/sales`
- **Verify:** see `_shared-contract.md` (Q15=C E2E + `cargo xtask test-affected` for `sales`)

## Revision

- Code SHA: fill at PR time (Q18=A). Every edit must update `docs/plan/traceability.md`. See `docs/plan/MAP.md` for flat index.
