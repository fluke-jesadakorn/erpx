# Context — identity-access

**Crate:** `crates/contexts/identity-access` · **Proto:** `crates/proto/proto/erpx/identity-access/service.proto` · **Adapters:** `crates/adapters/identity-access-sqlx` (where present) · **UI:** `apps/console/src/features/identity-access` / `crates/ui-domains/*` / `crates/console-features/identity-access`

## Services & RPCs

No `service.proto` — see `crates/domain` or `crates/contexts/identity-access/src/*.rs`.

Total RPCs: 0.

## Domain (sample)

Aggregates / types: —

Source: `src/domain.rs` (and `src/application.rs` —).

## Ports & adapters

- Ports: `src/ports.rs`
- Application: `src/application.rs`
- Persistence: `crates/adapters/identity-access-sqlx` (SQLx, CQRS read/write split where applicable)

## Invariants

Enforced in `crates/domain` and `crates/contexts/identity-access/src/domain.rs` — validate via `cargo xtask test-affected` and `crates/contexts/identity-access` unit tests. Cross-context rules travel only through versioned proto contracts, never direct domain imports.
