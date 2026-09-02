# Benefits — Deep `@` + AI (Production)

**Group:** `11-talent-learning-benefits` · **Code:** `crates/contexts/benefits` · **Proto:** `crates/proto/proto/erpx/benefits/service.proto` (`BenefitsCommandService, BenefitsQueryService` · 9 RPCs) · **Adapters:** `crates/adapters/benefits-sqlx` · **UI:** `apps/console/src/features/benefits` / `crates/ui-domains/*` · **Migrations:** `crates/migration/migrations/000*_*.sql`

**Scope:** Extends `docs/plan/02-capabilities/11-talent-learning-benefits/benefits.md`. Central polymorphic `@` contract in `docs/plan/01-vision-and-scope.md` applies verbatim — see `_shared-contract.md` for @ tokenizer, AI lifecycle, Forms, Verify, Spine, ASCII frame.

## Services & RPCs (9)

- **Services:** BenefitsCommandService, BenefitsQueryService
- **RPCs:** ** PublishPlan, OpenEnrollment, AddDependent, ElectCoverage, SubmitProvider, AcknowledgeProvider, ReleasePayroll, GetEnrollment, GetRecord

| RPC | Service | Data touch |
|-----|---------|------------|
| `** PublishPlan` | Command/Query | `benefits` aggregate + outbox |
| `OpenEnrollment` | Command/Query | `benefits` aggregate + outbox |
| `AddDependent` | Command/Query | `benefits` aggregate + outbox |
| `ElectCoverage` | Command/Query | `benefits` aggregate + outbox |
| `SubmitProvider` | Command/Query | `benefits` aggregate + outbox |
| `AcknowledgeProvider` | Command/Query | `benefits` aggregate + outbox |
| `ReleasePayroll` | Command/Query | `benefits` aggregate + outbox |
| `GetEnrollment` | Command/Query | `benefits` projection |
| `GetRecord` | Command/Query | `benefits` projection |

## Invariants (domain — keep)

- Enrollment windows are gated; coverage requires dependent validation.

Generic 3 (holding-isolation / time-bounded / append-only) — see `_shared-contract.md`.

## Data owned (delta)

- Write: `benefits` aggregate + `outbox_events` (CQRS, ADR-002) in holding/branch pair
- Read: rebuildable projection for search; `benefits enrollments + dependents` (light `benefits.md:16` canonical)
- Reference graph: see `_shared-contract.md` `record_reference_edges` definition

## Editable fields & schema binding

- **This module:** ** no owner-published schema — OwnerUnsupported (spec-ahead candidate)
- Binding: see `_shared-contract.md` (`MAX_FIELD_PATCH_OPERATIONS=64`, `StableFieldRef`, etc.)

## Business flows (unique)

1. **Happy:** PublishPlan → OpenEnrollment → ElectCoverage → Submit/Acknowledge provider → ReleasePayroll — creates typed edges (e.g., `@sales/…`, `@finance/…`) and is queryable via `search`/`backlinks`.
2. **Exception:** see original deep — validates invariants + degraded `Stale/Degraded`.
3. **AI-assisted:** see `_shared-contract.md` (pins ≥3 refs → ghost → Apply/Conflicted).

## Route & UI delta

- **Route:** `/benefits + /people` + `/work + /reference/item/:id`
- Shared frame: see `_shared-contract.md` ASCII (title BENEFITS, list benefits-001/002)

## Traceability

- **Proto:** `crates/proto/proto/erpx/benefits/service.proto` + `reference/service.proto`
- **Domain:** `crates/contexts/benefits/src/domain.rs`
- **Adapters:** `benefits-sqlx`
- **UI:** `apps/console/src/features/benefits`
- **Verify:** see `_shared-contract.md` (Q15=C E2E + `cargo xtask test-affected` for `benefits`)

## Revision

- Code SHA: fill at PR time (Q18=A). See `docs/plan/MAP.md` for flat index.
