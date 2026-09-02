# Projects — Deep `@` + AI (Production)

**Group:** `09-projects` · **Code:** `crates/contexts/projects` · **Proto:** `crates/proto/proto/erpx/projects/service.proto` (`ProjectsCommandService, ProjectsQueryService` · 23 RPCs) · **Adapters:** `crates/adapters/projects-sqlx` · **UI:** `apps/console/src/features/projects` / `crates/ui-domains/*` · **Migrations:** `crates/migration/migrations/000*_*.sql`

**Scope:** Extends `docs/plan/02-capabilities/09-projects/projects.md`. Central polymorphic `@` contract in `docs/plan/01-vision-and-scope.md` applies verbatim — see `_shared-contract.md` for @ tokenizer, AI lifecycle, Forms, Verify, Spine, ASCII frame.

## Services & RPCs (23)

- **Services:** ProjectsCommandService, ProjectsQueryService
- **RPCs:** ** CreateProject, TransitionProject, AddNode, MoveNode, TransitionNode, AttachNodeReference, AddNodeAttachment, RecordNodeTime, RecordNodeCost, ProposeDependency, DecideDependency, CancelDependency, RevokeDependency, ProposeHandoff, ApproveHandoff, MarkHandoffReady, MarkHandoffUncertain, CancelHandoff, ReconcileHandoff, GetProject, GetProjectGraph, ListProjects, GetRecord

| RPC | Service | Data touch |
|-----|---------|------------|
| `** CreateProject` | Command/Query | `projects` aggregate + outbox |
| `TransitionProject` | Command/Query | `projects` aggregate + outbox |
| `AddNode` | Command/Query | `projects` aggregate + outbox |
| `MoveNode` | Command/Query | `projects` aggregate + outbox |
| `TransitionNode` | Command/Query | `projects` aggregate + outbox |
| `AttachNodeReference` | Command/Query | `projects` aggregate + outbox |
| `AddNodeAttachment` | Command/Query | `projects` aggregate + outbox |
| `RecordNodeTime` | Command/Query | `projects` aggregate + outbox |
| `RecordNodeCost` | Command/Query | `projects` aggregate + outbox |
| `ProposeDependency` | Command/Query | `projects` aggregate + outbox |
| `DecideDependency` | Command/Query | `projects` aggregate + outbox |
| `CancelDependency` | Command/Query | `projects` aggregate + outbox |

## Invariants (domain — keep)

- Projects own WBS and resource assignments; billing via commercial.

Generic 3 (holding-isolation / time-bounded / append-only) — see `_shared-contract.md`.

## Data owned (delta)

- Write: `projects` aggregate + `outbox_events` (CQRS, ADR-002) in holding/branch pair
- Read: rebuildable projection for search; `projects WBS` (light `projects.md:16` canonical)
- Reference graph: see `_shared-contract.md` `record_reference_edges` definition

## Editable fields & schema binding

- **This module:** ** no owner-published schema — OwnerUnsupported (spec-ahead candidate)
- Binding: see `_shared-contract.md` (`MAX_FIELD_PATCH_OPERATIONS=64`, `StableFieldRef`, etc.)

## Business flows (unique)

1. **Happy:** CreateProject → Add/Move nodes → dependencies → handoffs → RecordTime/Cost — creates typed edges (e.g., `@sales/…`, `@finance/…`) and is queryable via `search`/`backlinks`.
2. **Exception:** see original deep — validates invariants + degraded `Stale/Degraded`.
3. **AI-assisted:** see `_shared-contract.md` (pins ≥3 refs → ghost → Apply/Conflicted).

## Route & UI delta

- **Route:** `/project + /work` + `/work + /reference/item/:id`
- Shared frame: see `_shared-contract.md` ASCII (title PROJECTS, list projects-001/002)

## Traceability

- **Proto:** `crates/proto/proto/erpx/projects/service.proto` + `reference/service.proto`
- **Domain:** `crates/contexts/projects/src/domain.rs`
- **Adapters:** `projects-sqlx`
- **UI:** `apps/console/src/features/projects`
- **Verify:** see `_shared-contract.md` (Q15=C E2E + `cargo xtask test-affected` for `projects`)

## Revision

- Code SHA: fill at PR time (Q18=A). See `docs/plan/MAP.md` for flat index.
