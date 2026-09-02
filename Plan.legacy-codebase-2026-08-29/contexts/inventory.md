# Context — inventory

**Crate:** `crates/contexts/inventory` · **Proto:** `crates/proto/proto/erpx/inventory/service.proto` · **Adapters:** `crates/adapters/inventory-sqlx` (where present) · **UI:** `apps/console/src/features/inventory` / `crates/ui-domains/*` / `crates/console-features/inventory`

## Services & RPCs

Register, Receive, Issue, Reserve, ReleaseReservation, Get, List

Total RPCs: 7.

## Domain (sample)

Aggregates / types: —

Source: `src/domain.rs` (and `src/application.rs` —).

## Ports & adapters

- Ports: `src/ports.rs`
- Application: `src/application.rs`
- Persistence: `crates/adapters/inventory-sqlx` (SQLx, CQRS read/write split where applicable)

## Invariants

Enforced in `crates/domain` and `crates/contexts/inventory/src/domain.rs` — validate via `cargo xtask test-affected` and `crates/contexts/inventory` unit tests. Cross-context rules travel only through versioned proto contracts, never direct domain imports.
