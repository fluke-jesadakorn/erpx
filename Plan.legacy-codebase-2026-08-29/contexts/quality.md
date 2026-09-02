# Context — quality

**Crate:** `crates/contexts/quality` · **Proto:** `crates/proto/proto/erpx/quality/service.proto` · **Adapters:** `crates/adapters/quality-sqlx` (where present) · **UI:** `apps/console/src/features/quality` / `crates/ui-domains/*` / `crates/console-features/quality`

## Services & RPCs

Create, Start, RecordResult, Release, Cancel, Get, List

Total RPCs: 7.

## Domain (sample)

Aggregates / types: —

Source: `src/domain.rs` (and `src/application.rs` —).

## Ports & adapters

- Ports: `src/ports.rs`
- Application: `src/application.rs`
- Persistence: `crates/adapters/quality-sqlx` (SQLx, CQRS read/write split where applicable)

## Invariants

Enforced in `crates/domain` and `crates/contexts/quality/src/domain.rs` — validate via `cargo xtask test-affected` and `crates/contexts/quality` unit tests. Cross-context rules travel only through versioned proto contracts, never direct domain imports.
