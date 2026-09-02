# Context — agent-orchestration

**Crate:** `crates/contexts/agent-orchestration` · **Proto:** `crates/proto/proto/erpx/agent-orchestration/service.proto` · **Adapters:** `crates/adapters/agent-orchestration-sqlx` (where present) · **UI:** `apps/console/src/features/agent-orchestration` / `crates/ui-domains/*` / `crates/console-features/agent-orchestration`

## Services & RPCs

No `service.proto` — see `crates/domain` or `crates/contexts/agent-orchestration/src/*.rs`.

Total RPCs: 0.

## Domain (sample)

Aggregates / types: AgentDomainError

Source: `src/domain.rs` (and `src/application.rs` present).

## Ports & adapters

- Ports: `src/ports.rs`
- Application: `src/application.rs`
- Persistence: `crates/adapters/agent-orchestration-sqlx` (SQLx, CQRS read/write split where applicable)

## Invariants

Enforced in `crates/domain` and `crates/contexts/agent-orchestration/src/domain.rs` — validate via `cargo xtask test-affected` and `crates/contexts/agent-orchestration` unit tests. Cross-context rules travel only through versioned proto contracts, never direct domain imports.
