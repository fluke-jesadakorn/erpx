# Context — sales

**Crate:** `crates/contexts/sales` · **Proto:** `crates/proto/proto/erpx/sales/service.proto` · **Adapters:** `crates/adapters/sales-sqlx` (where present) · **UI:** `apps/console/src/features/sales` / `crates/ui-domains/*` / `crates/console-features/sales`

## Services & RPCs

CreateQuote, SendQuote, AcceptQuote, RejectQuote, ExpireQuote, CreateSalesOrder, ConfirmSalesOrder, MarkSalesOrderPartiallyFulfilled, MarkSalesOrderFulfilled, CancelSalesOrder, GetQuote, GetSalesOrder, ListSalesOrders

Total RPCs: 13.

## Domain (sample)

Aggregates / types: —

Source: `src/domain.rs` (and `src/application.rs` —).

## Ports & adapters

- Ports: `src/ports.rs`
- Application: `src/application.rs`
- Persistence: `crates/adapters/sales-sqlx` (SQLx, CQRS read/write split where applicable)

## Invariants

Enforced in `crates/domain` and `crates/contexts/sales/src/domain.rs` — validate via `cargo xtask test-affected` and `crates/contexts/sales` unit tests. Cross-context rules travel only through versioned proto contracts, never direct domain imports.
