# Context — operations

**Crate:** `crates/contexts/operations` · **Proto:** `crates/proto/proto/erpx/operations/service.proto` · **Adapters:** `crates/adapters/operations-sqlx` (where present) · **UI:** `apps/console/src/features/operations` / `crates/ui-domains/*` / `crates/console-features/operations`

## Services & RPCs

ReassignException, AcknowledgeException, EscalateException, DecideApproval, ResolveException, CloseException, PublishWorkQueuePolicy, GetException, ListExceptionQueue, ListWorkQueuePolicies, GetSummary

Total RPCs: 11.

## Domain (sample)

Aggregates / types: —

Source: `src/domain.rs` (and `src/application.rs` —).

## Ports & adapters

- Ports: `src/ports.rs`
- Application: `src/application.rs`
- Persistence: `crates/adapters/operations-sqlx` (SQLx, CQRS read/write split where applicable)

## Invariants

Enforced in `crates/domain` and `crates/contexts/operations/src/domain.rs` — validate via `cargo xtask test-affected` and `crates/contexts/operations` unit tests. Cross-context rules travel only through versioned proto contracts, never direct domain imports.
