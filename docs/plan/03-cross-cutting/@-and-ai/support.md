# Support — Deep `@` + AI (Production)

**Group:** `02-customer-support` · **Code:** `crates/contexts/support` · **Proto:** `crates/proto/proto/erpx/support/service.proto` (`SupportCommandService, SupportQueryService` · 21 RPCs) · **Adapters:** `crates/adapters/support-sqlx` · **UI:** `apps/console/src/features/support` / `crates/ui-domains/*` · **Migrations:** `crates/migration/migrations/000*_*.sql`

**Scope:** Extends `docs/plan/02-capabilities/02-customer-support/support.md`. Central polymorphic `@` contract in `docs/plan/01-vision-and-scope.md` applies verbatim — see `_shared-contract.md` for @ tokenizer, AI lifecycle, Forms, Verify, Spine, ASCII frame.

## Services & RPCs (21)

- **Services:** SupportCommandService, SupportQueryService
- **RPCs:** ** OpenTicket, TransitionTicket, AddCustomerReply, AddInternalNote, RequestSession, ApproveSession, RevokeSession, IssueCapability, RevokeCapability, OpenIncident, TransitionIncident, AddIncidentTimeline, BroadcastIncident, GetCustomerTicket, ListCustomerTickets, GetInternalTicket, ListInternalTickets, GetIncident, ListIncidents, GetSession, GetCapability

| RPC | Service | Data touch |
|-----|---------|------------|
| `** OpenTicket` | Command/Query | `support` aggregate + outbox |
| `TransitionTicket` | Command/Query | `support` aggregate + outbox |
| `AddCustomerReply` | Command/Query | `support` aggregate + outbox |
| `AddInternalNote` | Command/Query | `support` aggregate + outbox |
| `RequestSession` | Command/Query | `support` aggregate + outbox |
| `ApproveSession` | Command/Query | `support` aggregate + outbox |
| `RevokeSession` | Command/Query | `support` aggregate + outbox |
| `IssueCapability` | Command/Query | `support` aggregate + outbox |
| `RevokeCapability` | Command/Query | `support` aggregate + outbox |
| `OpenIncident` | Command/Query | `support` aggregate + outbox |
| `TransitionIncident` | Command/Query | `support` aggregate + outbox |
| `AddIncidentTimeline` | Command/Query | `support` aggregate + outbox |

## Invariants (domain — keep)

- Support tickets/sessions are customer-scoped with capability-gated access and SLA clocks.

Generic 3 (holding-isolation / time-bounded / append-only) — see `_shared-contract.md`.

## Data owned (delta)

- Write: `support` aggregate + `outbox_events` (CQRS, ADR-002) in holding/branch pair
- Read: rebuildable projection for search; `support_tickets/support_sessions + support_capabilities + incidents` (light `support.md:16` canonical)
- Reference graph: see `_shared-contract.md` `record_reference_edges` definition

## Editable fields & schema binding

- **This module:** ** no owner-published schema — OwnerUnsupported (spec-ahead candidate)
- Binding: see `_shared-contract.md` (`MAX_FIELD_PATCH_OPERATIONS=64`, `StableFieldRef`, etc.)

## Business flows (unique)

1. **Happy:** Open ticket → triage → resolve/close with SLA — creates typed edges (e.g., `@sales/…`, `@finance/…`) and is queryable via `search`/`backlinks`.
2. **Exception:** see original deep — validates invariants + degraded `Stale/Degraded`.
3. **AI-assisted:** see `_shared-contract.md` (pins ≥3 refs → ghost → Apply/Conflicted).

## Route & UI delta

- **Route:** `/platform/support + /work + /reference/item/:id` + `/work + /reference/item/:id`
- Shared frame: see `_shared-contract.md` ASCII (title SUPPORT, list support-001/002)

## Traceability

- **Proto:** `crates/proto/proto/erpx/support/service.proto` + `reference/service.proto`
- **Domain:** `crates/contexts/support/src/domain.rs`
- **Adapters:** `support-sqlx`
- **UI:** `apps/console/src/features/support`
- **Verify:** see `_shared-contract.md` (Q15=C E2E + `cargo xtask test-affected` for `support`)

## Revision

- Code SHA: fill at PR time (Q18=A). See `docs/plan/MAP.md` for flat index.
