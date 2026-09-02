# Context — assets

**Crate:** `crates/contexts/assets` · **Proto:** `crates/proto/proto/erpx/assets/service.proto` · **Adapters:** `crates/adapters/assets-sqlx` (where present) · **UI:** `apps/console/src/features/assets` / `crates/ui-domains/*` / `crates/console-features/assets`

## Services & RPCs

Create, PlaceInService, MarkDegraded, BeginMaintenance, ReturnToService, Retire, Get, List

Total RPCs: 8.

## Domain (sample)

Aggregates / types: —

Source: `src/domain.rs` (and `src/application.rs` —).

## Ports & adapters

- Ports: `src/ports.rs`
- Application: `src/application.rs`
- Persistence: `crates/adapters/assets-sqlx` (SQLx, CQRS read/write split where applicable)

## Invariants

Enforced in `crates/domain` and `crates/contexts/assets/src/domain.rs` — validate via `cargo xtask test-affected` and `crates/contexts/assets` unit tests. Cross-context rules travel only through versioned proto contracts, never direct domain imports.
