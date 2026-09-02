# Context — procurement

**Crate:** `crates/contexts/procurement` · **Proto:** `crates/proto/proto/erpx/procurement/service.proto` · **Adapters:** `crates/adapters/procurement-sqlx` (where present) · **UI:** `apps/console/src/features/procurement` / `crates/ui-domains/*` / `crates/console-features/procurement`

## Services & RPCs

Create, Release, Cancel, CreateRequisition, SubmitRequisition, ApproveRequisition, RejectRequisition, CancelRequisition, CreateGoodsReceipt, AcceptGoodsReceipt, RejectGoodsReceipt, CancelGoodsReceipt, Get, List, GetRequisition, GetGoodsReceipt

Total RPCs: 16.

## Domain (sample)

Aggregates / types: —

Source: `src/domain.rs` (and `src/application.rs` —).

## Ports & adapters

- Ports: `src/ports.rs`
- Application: `src/application.rs`
- Persistence: `crates/adapters/procurement-sqlx` (SQLx, CQRS read/write split where applicable)

## Invariants

Enforced in `crates/domain` and `crates/contexts/procurement/src/domain.rs` — validate via `cargo xtask test-affected` and `crates/contexts/procurement` unit tests. Cross-context rules travel only through versioned proto contracts, never direct domain imports.
