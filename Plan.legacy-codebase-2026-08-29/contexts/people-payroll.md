# Context — people-payroll

**Crate:** `crates/contexts/people-payroll` · **Proto:** `crates/proto/proto/erpx/people-payroll/service.proto` · **Adapters:** `crates/adapters/people-payroll-sqlx` (where present) · **UI:** `apps/console/src/features/people-payroll` / `crates/ui-domains/*` / `crates/console-features/people-payroll`

## Services & RPCs

No `service.proto` — see `crates/domain` or `crates/contexts/people-payroll/src/*.rs`.

Total RPCs: 0.

## Domain (sample)

Aggregates / types: —

Source: `src/domain.rs` (and `src/application.rs` —).

## Ports & adapters

- Ports: `src/ports.rs`
- Application: `src/application.rs`
- Persistence: `crates/adapters/people-payroll-sqlx` (SQLx, CQRS read/write split where applicable)

## Invariants

Enforced in `crates/domain` and `crates/contexts/people-payroll/src/domain.rs` — validate via `cargo xtask test-affected` and `crates/contexts/people-payroll` unit tests. Cross-context rules travel only through versioned proto contracts, never direct domain imports.
