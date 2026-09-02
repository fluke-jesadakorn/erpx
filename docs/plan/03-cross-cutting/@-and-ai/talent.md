# Talent — Deep `@` + AI (Production)

**Group:** `11-talent-learning-benefits` · **Code:** `crates/contexts/talent` · **Proto:** `crates/proto/proto/erpx/talent/service.proto` (`TalentCommandService, TalentQueryService` · 28 RPCs) · **Adapters:** `crates/adapters/talent-sqlx` · **UI:** `apps/console/src/features/talent` / `crates/ui-domains/*` · **Migrations:** `crates/migration/migrations/000*_*.sql`

**Scope:** Extends `docs/plan/02-capabilities/11-talent-learning-benefits/talent.md`. Central polymorphic `@` contract in `docs/plan/01-vision-and-scope.md` applies verbatim — see `_shared-contract.md` for @ tokenizer, AI lifecycle, Forms, Verify, Spine, ASCII frame.

## Services & RPCs (28)

- **Services:** TalentCommandService, TalentQueryService
- **RPCs:** ** CreateRequisition, ApproveRequisition, TransitionRequisition, CreateCandidateProfile, RequestCandidateErasure, CompleteCandidateErasure, SetCandidateLegalHold, SubmitApplication, AddApplicationInterview, SelectApplication, WithdrawApplication, CreateOffer, ApproveOffer, PresentOffer, DecideOffer, RequestHireHandoff, ReconcileHireHandoff, CreateProposal, SubmitProposal, ApproveProposal, ReconcileProposal, GetRequisition, GetCandidateProfile, GetApplication, GetOffer, GetHireHandoff, GetProposal, GetRecord

| RPC | Service | Data touch |
|-----|---------|------------|
| `** CreateRequisition` | Command/Query | `talent` aggregate + outbox |
| `ApproveRequisition` | Command/Query | `talent` aggregate + outbox |
| `TransitionRequisition` | Command/Query | `talent` aggregate + outbox |
| `CreateCandidateProfile` | Command/Query | `talent` aggregate + outbox |
| `RequestCandidateErasure` | Command/Query | `talent` aggregate + outbox |
| `CompleteCandidateErasure` | Command/Query | `talent` aggregate + outbox |
| `SetCandidateLegalHold` | Command/Query | `talent` aggregate + outbox |
| `SubmitApplication` | Command/Query | `talent` aggregate + outbox |
| `AddApplicationInterview` | Command/Query | `talent` aggregate + outbox |
| `SelectApplication` | Command/Query | `talent` aggregate + outbox |
| `WithdrawApplication` | Command/Query | `talent` aggregate + outbox |
| `CreateOffer` | Command/Query | `talent` aggregate + outbox |

## Invariants (domain — keep)

- Talent pipelines are auditable; decisions carry evidence.

Generic 3 (holding-isolation / time-bounded / append-only) — see `_shared-contract.md`.

## Data owned (delta)

- Write: `talent` aggregate + `outbox_events` (CQRS, ADR-002) in holding/branch pair
- Read: rebuildable projection for search; `talent pipelines` (light `talent.md:16` canonical)
- Reference graph: see `_shared-contract.md` `record_reference_edges` definition

## Editable fields & schema binding

- **This module:** ** no owner-published schema — OwnerUnsupported (spec-ahead candidate)
- Binding: see `_shared-contract.md` (`MAX_FIELD_PATCH_OPERATIONS=64`, `StableFieldRef`, etc.)

## Business flows (unique)

1. **Happy:** CreateRequisition → Approve → Application → Interview → Offer → Hire handoff → Proposal — creates typed edges (e.g., `@sales/…`, `@finance/…`) and is queryable via `search`/`backlinks`.
2. **Exception:** see original deep — validates invariants + degraded `Stale/Degraded`.
3. **AI-assisted:** see `_shared-contract.md` (pins ≥3 refs → ghost → Apply/Conflicted).

## Route & UI delta

- **Route:** `/talent + /people` + `/work + /reference/item/:id`
- Shared frame: see `_shared-contract.md` ASCII (title TALENT, list talent-001/002)

## Traceability

- **Proto:** `crates/proto/proto/erpx/talent/service.proto` + `reference/service.proto`
- **Domain:** `crates/contexts/talent/src/domain.rs`
- **Adapters:** `talent-sqlx`
- **UI:** `apps/console/src/features/talent`
- **Verify:** see `_shared-contract.md` (Q15=C E2E + `cargo xtask test-affected` for `talent`)

## Revision

- Code SHA: fill at PR time (Q18=A). See `docs/plan/MAP.md` for flat index.
