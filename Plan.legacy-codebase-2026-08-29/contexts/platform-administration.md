# Context — platform-administration

**Crate:** `crates/contexts/platform-administration` · **Proto:** `crates/proto/proto/erpx/platform-administration/service.proto` · **Adapters:** `crates/adapters/platform-administration-sqlx` (where present) · **UI:** `apps/console/src/features/platform-administration` / `crates/ui-domains/*` / `crates/console-features/platform-administration`

## Services & RPCs

No `service.proto` — see `crates/domain` or `crates/contexts/platform-administration/src/*.rs`.

Total RPCs: 0.

## Domain (sample)

Aggregates / types: —

Source: `src/domain.rs` (and `src/application.rs` —).

## Ports & adapters

- Ports: `src/ports.rs`
- Application: `src/application.rs`
- Persistence: `crates/adapters/platform-administration-sqlx` (SQLx, CQRS read/write split where applicable)

## Invariants

Enforced in `crates/domain` and `crates/contexts/platform-administration/src/domain.rs` — validate via `cargo xtask test-affected` and `crates/contexts/platform-administration` unit tests. Cross-context rules travel only through versioned proto contracts, never direct domain imports.
