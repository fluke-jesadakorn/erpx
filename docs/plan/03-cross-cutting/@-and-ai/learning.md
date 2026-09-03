# Learning — Deep `@` + AI (Production)

**Group:** `11-talent-learning-benefits` · **Code:** `crates/contexts/learning` · **Proto:** `crates/proto/proto/erpx/learning/service.proto` (`LearningCommandService, LearningQueryService` · 9 RPCs) · **Adapters:** `crates/adapters/learning-sqlx` · **UI:** `apps/console/src/features/learning` / `crates/ui-domains/*` · **Migrations:** `crates/migration/migrations/000*_*.sql`

**Scope:** Extends `docs/plan/02-capabilities/11-talent-learning-benefits/learning.md`. Central polymorphic `@` contract in `docs/plan/01-vision-and-scope.md` applies verbatim — see `_shared-contract.md` for @ tokenizer, AI lifecycle, Forms, Verify, Spine, ASCII frame.

## Services & RPCs (9)

- **Services:** LearningCommandService, LearningQueryService
- **RPCs:** ** PublishCourse, RetireCourse, AssignCourse, RecordAttendance, RecordProviderResult, WaiveEnrollment, CompleteEnrollment, GetEnrollment, GetRecord

| RPC | Service | Data touch |
|-----|---------|------------|
| `** PublishCourse` | Command/Query | `learning` aggregate + outbox |
| `RetireCourse` | Command/Query | `learning` aggregate + outbox |
| `AssignCourse` | Command/Query | `learning` aggregate + outbox |
| `RecordAttendance` | Command/Query | `learning` aggregate + outbox |
| `RecordProviderResult` | Command/Query | `learning` aggregate + outbox |
| `WaiveEnrollment` | Command/Query | `learning` aggregate + outbox |
| `CompleteEnrollment` | Command/Query | `learning` aggregate + outbox |
| `GetEnrollment` | Command/Query | `learning` projection |
| `GetRecord` | Command/Query | `learning` projection |

## Invariants (domain — keep)

- Learning enrollments are idempotent; completion drives benefits eligibility.

Generic 3 (holding-isolation / time-bounded / append-only) — see `_shared-contract.md`.

## Data owned (delta)

- Write: `learning` aggregate + `outbox_events` (CQRS, ADR-002) in holding/branch pair
- Read: rebuildable projection for search; `learning enrollments` (light `learning.md:16` canonical)
- Reference graph: see `_shared-contract.md` `record_reference_edges` definition

## Editable fields & schema binding

- **This module:** ** no owner-published schema — OwnerUnsupported (spec-ahead candidate)
- Binding: see `_shared-contract.md` (`MAX_FIELD_PATCH_OPERATIONS=64`, `StableFieldRef`, etc.)

## Business flows (unique)

1. **Happy:** PublishCourse → Assign → Attendance/Result → Complete/Waive/Retire — creates typed edges (e.g., `@sales/…`, `@finance/…`) and is queryable via `search`/`backlinks`.
2. **Exception:** see original deep — validates invariants + degraded `Stale/Degraded`.
3. **AI-assisted:** see `_shared-contract.md` (pins ≥3 refs → ghost → Apply/Conflicted).

## Feed mapping (paradigm: `docs/plan/04-social-paradigm.md`)

- **Emit:** `PublishCourse`→post; completions→celebration posts; overdue mandatory→lane (`people-lead`).
- **Comments:** open; completions drive benefits eligibility visibly.

## Route & UI delta

- **Route:** `/learning + /talent` + `/work + /reference/item/:id`
- Shared frame: see `_shared-contract.md` ASCII (title LEARNING, list learning-001/002)

## Traceability

- **Proto:** `crates/proto/proto/erpx/learning/service.proto` + `reference/service.proto`
- **Domain:** `crates/contexts/learning/src/domain.rs`
- **Adapters:** `learning-sqlx`
- **UI:** `apps/console/src/features/learning`
- **Verify:** see `_shared-contract.md` (Q15=C E2E + `cargo xtask test-affected` for `learning`)

## Revision

- Code SHA: fill at PR time (Q18=A). See `docs/plan/MAP.md` for flat index.
