# Context — learning

**Crate:** `crates/contexts/learning` · **Proto:** `crates/proto/proto/erpx/learning/service.proto` · **Adapters:** `crates/adapters/learning-sqlx` (where present) · **UI:** `apps/console/src/features/learning` / `crates/ui-domains/*` / `crates/console-features/learning`

## Services & RPCs

PublishCourse, RetireCourse, AssignCourse, RecordAttendance, RecordProviderResult, WaiveEnrollment, CompleteEnrollment, GetEnrollment, GetRecord

Total RPCs: 9.

## Domain (sample)

Aggregates / types: CourseStatus, LearningHumanDecisionEvidence

Source: `src/domain.rs` (and `src/application.rs` present).

## Ports & adapters

- Ports: `src/ports.rs`
- Application: `src/application.rs`
- Persistence: `crates/adapters/learning-sqlx` (SQLx, CQRS read/write split where applicable)

## Invariants

Enforced in `crates/domain` and `crates/contexts/learning/src/domain.rs` — validate via `cargo xtask test-affected` and `crates/contexts/learning` unit tests. Cross-context rules travel only through versioned proto contracts, never direct domain imports.
