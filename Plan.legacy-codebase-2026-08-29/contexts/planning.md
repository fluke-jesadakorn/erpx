# Context — planning

**Crate:** `crates/contexts/planning` · **Proto:** `crates/proto/proto/erpx/planning/service.proto` · **Adapters:** `crates/adapters/planning-sqlx` (where present) · **UI:** `apps/console/src/features/planning` / `crates/ui-domains/*` / `crates/console-features/planning`

## Services & RPCs

CreateScenario, FreezeScenario, RunScenario, OverrideRecommendation, ApproveRecommendation, RejectRecommendation, ReleaseRecommendation, GetScenario, GetRecommendation, GetRun, ListRecommendations, ListShortages, ListCapacityOverloads, GetPlan

Total RPCs: 14.

## Domain (sample)

Aggregates / types: —

Source: `src/domain.rs` (and `src/application.rs` present).

## Ports & adapters

- Ports: `src/ports.rs`
- Application: `src/application.rs`
- Persistence: `crates/adapters/planning-sqlx` (SQLx, CQRS read/write split where applicable)

## Invariants

Enforced in `crates/domain` and `crates/contexts/planning/src/domain.rs` — validate via `cargo xtask test-affected` and `crates/contexts/planning` unit tests. Cross-context rules travel only through versioned proto contracts, never direct domain imports.
