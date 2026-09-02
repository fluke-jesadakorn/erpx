# Context — customer-360

**Crate:** `crates/contexts/customer-360` · **Proto:** `crates/proto/proto/erpx/customer-360/service.proto` · **Adapters:** `crates/adapters/customer-360-sqlx` (where present) · **UI:** `apps/console/src/features/customer-360` / `crates/ui-domains/*` / `crates/console-features/customer-360`

## Services & RPCs

No `service.proto` — see `crates/domain` or `crates/contexts/customer-360/src/*.rs`.

Total RPCs: 0.

## Domain (sample)

Aggregates / types: —

Source: `src/domain.rs` (and `src/application.rs` —).

## Ports & adapters

- Ports: `src/ports.rs`
- Application: `src/application.rs`
- Persistence: `crates/adapters/customer-360-sqlx` (SQLx, CQRS read/write split where applicable)

## Invariants

Enforced in `crates/domain` and `crates/contexts/customer-360/src/domain.rs` — validate via `cargo xtask test-affected` and `crates/contexts/customer-360` unit tests. Cross-context rules travel only through versioned proto contracts, never direct domain imports.
