# Context — projects

**Crate:** `crates/contexts/projects` · **Proto:** `crates/proto/proto/erpx/projects/service.proto` · **Adapters:** `crates/adapters/projects-sqlx` (where present) · **UI:** `apps/console/src/features/projects` / `crates/ui-domains/*` / `crates/console-features/projects`

## Services & RPCs

CreateProject, TransitionProject, AddNode, MoveNode, TransitionNode, AttachNodeReference, AddNodeAttachment, RecordNodeTime, RecordNodeCost, ProposeDependency, DecideDependency, CancelDependency, RevokeDependency, ProposeHandoff, ApproveHandoff, MarkHandoffReady, MarkHandoffUncertain, CancelHandoff, ReconcileHandoff, GetProject, GetProjectGraph, ListProjects, GetRecord

Total RPCs: 23.

## Domain (sample)

Aggregates / types: —

Source: `src/domain.rs` (and `src/application.rs` —).

## Ports & adapters

- Ports: `src/ports.rs`
- Application: `src/application.rs`
- Persistence: `crates/adapters/projects-sqlx` (SQLx, CQRS read/write split where applicable)

## Invariants

Enforced in `crates/domain` and `crates/contexts/projects/src/domain.rs` — validate via `cargo xtask test-affected` and `crates/contexts/projects` unit tests. Cross-context rules travel only through versioned proto contracts, never direct domain imports.
