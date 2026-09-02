# Context — brand

**Crate:** `crates/contexts/brand` · **Proto:** `crates/proto/proto/erpx/brand/service.proto` · **Adapters:** `crates/adapters/brand-sqlx` (where present) · **UI:** `apps/console/src/features/brand` / `crates/ui-domains/*` / `crates/console-features/brand`

## Services & RPCs

CreateDraft, SaveDraft, PublishDraft, Rollback, SetCategoryLocks, GetDraft, GetResolved, ListHistory

Total RPCs: 8.

## Domain (sample)

Aggregates / types: —

Source: `src/domain.rs` (and `src/application.rs` —).

## Ports & adapters

- Ports: `src/ports.rs`
- Application: `src/application.rs`
- Persistence: `crates/adapters/brand-sqlx` (SQLx, CQRS read/write split where applicable)

## Invariants

Enforced in `crates/domain` and `crates/contexts/brand/src/domain.rs` — validate via `cargo xtask test-affected` and `crates/contexts/brand` unit tests. Cross-context rules travel only through versioned proto contracts, never direct domain imports.
