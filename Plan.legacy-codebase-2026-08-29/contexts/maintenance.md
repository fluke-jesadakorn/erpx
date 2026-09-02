# Context — maintenance

**Crate:** `crates/contexts/maintenance` · **Proto:** `crates/proto/proto/erpx/maintenance/service.proto` · **Adapters:** `crates/adapters/maintenance-sqlx` (where present) · **UI:** `apps/console/src/features/maintenance` / `crates/ui-domains/*` / `crates/console-features/maintenance`

## Services & RPCs

Open, Schedule, Start, Block, Resume, Complete, Cancel, RecordDowntime, Get, List

Total RPCs: 10.

## Domain (sample)

Aggregates / types: —

Source: `src/domain.rs` (and `src/application.rs` —).

## Ports & adapters

- Ports: `src/ports.rs`
- Application: `src/application.rs`
- Persistence: `crates/adapters/maintenance-sqlx` (SQLx, CQRS read/write split where applicable)

## Invariants

Enforced in `crates/domain` and `crates/contexts/maintenance/src/domain.rs` — validate via `cargo xtask test-affected` and `crates/contexts/maintenance` unit tests. Cross-context rules travel only through versioned proto contracts, never direct domain imports.
