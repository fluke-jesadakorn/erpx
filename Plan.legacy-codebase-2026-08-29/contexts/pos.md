# Context — pos

**Crate:** `crates/contexts/pos` · **Proto:** `crates/proto/proto/erpx/pos/service.proto` · **Adapters:** `crates/adapters/pos-sqlx` (where present) · **UI:** `apps/console/src/features/pos` / `crates/ui-domains/*` / `crates/console-features/pos`

## Services & RPCs

CreateSale, OpenSale, HoldSale, ResumeSale, RecordPayment, CompleteSale, VoidSale, RequestRefund, ApproveRefund, RejectRefund, PostRefund, CancelRefund, GetSale, ListSales, GetRefund, ListRefunds

Total RPCs: 16.

## Domain (sample)

Aggregates / types: —

Source: `src/domain.rs` (and `src/application.rs` —).

## Ports & adapters

- Ports: `src/ports.rs`
- Application: `src/application.rs`
- Persistence: `crates/adapters/pos-sqlx` (SQLx, CQRS read/write split where applicable)

## Invariants

Enforced in `crates/domain` and `crates/contexts/pos/src/domain.rs` — validate via `cargo xtask test-affected` and `crates/contexts/pos` unit tests. Cross-context rules travel only through versioned proto contracts, never direct domain imports.
