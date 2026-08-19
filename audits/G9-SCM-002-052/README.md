# G9-SCM-002 — Procurement and Inventory domain contracts

## Delivered behavior

`erpx/crates/domain/src/procurement_inventory.rs` provides a transport- and
persistence-free Procurement/Inventory contract slice with:

- validated company/branch holding scope and command-scope containment;
- deterministic references and namespaced idempotency keys for command-owned
  Products, Warehouses, Purchase Requests, Purchase Orders, Goods Receipts,
  Stock Movements, Stock Transfers, and Stock Reservations;
- scoped `VendorReference`, `ProductReference`, and `WarehouseReference`;
- closed Purchase Request, Purchase Order, Goods Receipt, movement, transfer,
  and reservation lifecycle transitions;
- approval-gated Purchase Order creation and issued-order-only receipt posting;
- receipt-line binding, outstanding-quantity checks, and duplicate-posting
  rejection;
- integer quantities and integer currency minor-unit costs throughout;
- bounded on-hand, reserved, available, issue, receive, release, and fulfill
  rules that reject negative availability;
- deterministic weighted-average cost using checked `i128` arithmetic and
  half-up integer rounding, plus explicit recosting.

## Files changed

- `erpx/crates/domain/src/procurement_inventory.rs`
- `erpx/audits/G9-SCM-002-052/README.md`

No shared contract, export, infrastructure, migration, application, server,
worker, or UI files were changed.

## Contracts consumed

- `crate::contracts::CommandContext`
- `crate::contracts::HoldingScope`
- Existing `CommandContext::validate` and `HoldingScope::contains` behavior.

## Contracts produced

- `ScmError`, `ScmAggregateKind`, deterministic reference/idempotency helpers.
- `VendorReference`, `ProductReference`, `WarehouseReference`, `Product`,
  and `Warehouse`.
- Purchase Request/Order and Goods Receipt aggregates with statuses and lines.
- `WeightedAverageCost`, `StockBalance`, `StockMovement`, `StockTransfer`, and
  `StockReservation` contracts.

## Verification

- `rustfmt --edition 2024 --check erpx/crates/domain/src/procurement_inventory.rs` — PASS.
- `cargo fmt --all --check` from `erpx/` — PASS.
- Isolated compile/test harness including the new module — PASS, 8 tests.
- Isolated `cargo clippy --all-targets -- -D warnings` — PASS.
- Repository `cargo test -p erpx-domain` — PASS, 96 tests; this command does
  not collect the new module until the main agent exports it from `domain/src/lib.rs`.

## Required integration order

1. Main agent exports `pub mod procurement_inventory;` from
   `erpx/crates/domain/src/lib.rs`.
2. Main agent reruns `cargo fmt --all --check`, `cargo test -p erpx-domain`,
   and `cargo clippy -p erpx-domain --all-targets -- -D warnings` so the four
   embedded SCM tests are collected in the real crate.
3. Application and transport adapters map the contracts without widening
   scope; persistence adds uniqueness for idempotency and optimistic version
   checks.
4. Sales, Operations, Finance, and Inventory adapters consume only explicit
   handoff references/events.

## Unfinished work

This packet intentionally contains no persistence, migrations, repositories,
projections, gRPC, worker, UI, physical inventory ledger, supplier billing,
or end-to-end Procure-to-Pay integration. Those remain separate packets.

## Security concerns

The domain fails closed for invalid scopes, cross-company references, expired
or terminal lifecycle reuse, over-receipt, over-reservation, and insufficient
available stock. It carries no credentials, payment instruments, or holding
connection details. Persistence and transport must still enforce authoritative
authorization, idempotency uniqueness, optimistic concurrency, and audit
attribution.

## Rollback

Remove only the two files listed under “Files changed”. No shared contract,
database, or runtime rollback is required.
