# Context — customer-authority

**Crate:** `crates/contexts/customer-authority` · **Proto:** `crates/proto/proto/erpx/customer-authority/service.proto` · **Adapters:** `crates/adapters/customer-authority-sqlx` (where present) · **UI:** `apps/console/src/features/customer-authority` / `crates/ui-domains/*` / `crates/console-features/customer-authority`

## Services & RPCs

No `service.proto` — see `crates/domain` or `crates/contexts/customer-authority/src/*.rs`.

Total RPCs: 0.

## Domain (sample)

Aggregates / types: —

Source: `src/domain.rs` (and `src/application.rs` —).

## Ports & adapters

- Ports: `src/ports.rs`
- Application: `src/application.rs`
- Persistence: `crates/adapters/customer-authority-sqlx` (SQLx, CQRS read/write split where applicable)

## Invariants

Enforced in `crates/domain` and `crates/contexts/customer-authority/src/domain.rs` — validate via `cargo xtask test-affected` and `crates/contexts/customer-authority` unit tests. Cross-context rules travel only through versioned proto contracts, never direct domain imports.
