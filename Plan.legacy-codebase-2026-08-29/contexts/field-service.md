# Context — field-service

**Crate:** `crates/contexts/field-service` · **Proto:** `crates/proto/proto/erpx/field-service/service.proto` · **Adapters:** `crates/adapters/field-service-sqlx` (where present) · **UI:** `apps/console/src/features/field-service` / `crates/ui-domains/*` / `crates/console-features/field-service`

## Services & RPCs

No `service.proto` — see `crates/domain` or `crates/contexts/field-service/src/*.rs`.

Total RPCs: 0.

## Domain (sample)

Aggregates / types: FieldServiceHumanDecisionEvidence

Source: `src/domain.rs` (and `src/application.rs` present).

## Ports & adapters

- Ports: `src/ports.rs`
- Application: `src/application.rs`
- Persistence: `crates/adapters/field-service-sqlx` (SQLx, CQRS read/write split where applicable)

## Invariants

Enforced in `crates/domain` and `crates/contexts/field-service/src/domain.rs` — validate via `cargo xtask test-affected` and `crates/contexts/field-service` unit tests. Cross-context rules travel only through versioned proto contracts, never direct domain imports.
