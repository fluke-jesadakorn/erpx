# Context — crm

**Crate:** `crates/contexts/crm` · **Proto:** `crates/proto/proto/erpx/crm/service.proto` · **Adapters:** `crates/adapters/crm-sqlx` (where present) · **UI:** `apps/console/src/features/crm` / `crates/ui-domains/*` / `crates/console-features/crm`

## Services & RPCs

SaveCrmHandoff, SaveCrmOpportunity, RecordCrmAttribution, AcceptMarketingLead, CreateOpportunity, TransitionOpportunity, ListCrmReadRows, GetCrmLead, GetOpportunity, ListOpportunities

Total RPCs: 10.

## Domain (sample)

Aggregates / types: —

Source: `src/domain.rs` (and `src/application.rs` —).

## Ports & adapters

- Ports: `src/ports.rs`
- Application: `src/application.rs`
- Persistence: `crates/adapters/crm-sqlx` (SQLx, CQRS read/write split where applicable)

## Invariants

Enforced in `crates/domain` and `crates/contexts/crm/src/domain.rs` — validate via `cargo xtask test-affected` and `crates/contexts/crm` unit tests. Cross-context rules travel only through versioned proto contracts, never direct domain imports.
