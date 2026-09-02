# Context — support

**Crate:** `crates/contexts/support` · **Proto:** `crates/proto/proto/erpx/support/service.proto` · **Adapters:** `crates/adapters/support-sqlx` (where present) · **UI:** `apps/console/src/features/support` / `crates/ui-domains/*` / `crates/console-features/support`

## Services & RPCs

OpenTicket, TransitionTicket, AddCustomerReply, AddInternalNote, RequestSession, ApproveSession, RevokeSession, IssueCapability, RevokeCapability, OpenIncident, TransitionIncident, AddIncidentTimeline, BroadcastIncident, GetCustomerTicket, ListCustomerTickets, GetInternalTicket, ListInternalTickets, GetIncident, ListIncidents, GetSession, GetCapability

Total RPCs: 21.

## Domain (sample)

Aggregates / types: —

Source: `src/domain.rs` (and `src/application.rs` —).

## Ports & adapters

- Ports: `src/ports.rs`
- Application: `src/application.rs`
- Persistence: `crates/adapters/support-sqlx` (SQLx, CQRS read/write split where applicable)

## Invariants

Enforced in `crates/domain` and `crates/contexts/support/src/domain.rs` — validate via `cargo xtask test-affected` and `crates/contexts/support` unit tests. Cross-context rules travel only through versioned proto contracts, never direct domain imports.
