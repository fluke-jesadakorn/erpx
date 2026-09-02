# Context — benefits

**Crate:** `crates/contexts/benefits` · **Proto:** `crates/proto/proto/erpx/benefits/service.proto` · **Adapters:** `crates/adapters/benefits-sqlx` (where present) · **UI:** `apps/console/src/features/benefits` / `crates/ui-domains/*` / `crates/console-features/benefits`

## Services & RPCs

PublishPlan, OpenEnrollment, AddDependent, ElectCoverage, SubmitProvider, AcknowledgeProvider, ReleasePayroll, GetEnrollment, GetRecord

Total RPCs: 9.

## Domain (sample)

Aggregates / types: PlanStatus, ContributionRule, BenefitPlan

Source: `src/domain.rs` (and `src/application.rs` present).

## Ports & adapters

- Ports: `src/ports.rs`
- Application: `src/application.rs`
- Persistence: `crates/adapters/benefits-sqlx` (SQLx, CQRS read/write split where applicable)

## Invariants

Enforced in `crates/domain` and `crates/contexts/benefits/src/domain.rs` — validate via `cargo xtask test-affected` and `crates/contexts/benefits` unit tests. Cross-context rules travel only through versioned proto contracts, never direct domain imports.
