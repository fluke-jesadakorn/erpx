# Context — organization

**Crate:** `crates/contexts/organization` · **Proto:** `crates/proto/proto/erpx/organization/service.proto` · **Adapters:** `crates/adapters/organization-sqlx` (where present) · **UI:** `apps/console/src/features/organization` / `crates/ui-domains/*` / `crates/console-features/organization`

## Services & RPCs

CreateNode, RenameNode, ChangeNodeCode, ReparentNode, SuspendNode, ActivateNode, ArchiveNode, GetNode, ListNodes

Total RPCs: 9.

## Domain (sample)

Aggregates / types: —

Source: `src/domain.rs` (and `src/application.rs` —).

## Ports & adapters

- Ports: `src/ports.rs`
- Application: `src/application.rs`
- Persistence: `crates/adapters/organization-sqlx` (SQLx, CQRS read/write split where applicable)

## Invariants

Enforced in `crates/domain` and `crates/contexts/organization/src/domain.rs` — validate via `cargo xtask test-affected` and `crates/contexts/organization` unit tests. Cross-context rules travel only through versioned proto contracts, never direct domain imports.
