# G4-SUP-008 — Bounded support ticket queue read

## Scope

Add the authoritative, read-only ticket queue query needed by the platform and
holding Support surfaces.

## Delivered behavior

- Added `ListTickets` protobuf request/response and SupportService RPC.
- Added bounded deterministic persistence ordering by `updated_at DESC, id
  DESC`, optional status/customer filters, exclusive typed keyset cursors, and
  `has_more` metadata.
- Server authorization requires an active authenticated session, the
  `support.ticket.read` role, a query scope, consistency metadata, and a
  non-empty audit purpose.
- Customer scope containment is checked for every returned ticket before the
  transport DTO is emitted; internal notes never enter `Ticket` replies.
- The list action is recorded as a customer-visible audit event.

## Evidence

- `cargo check -p erpx-server` — PASS.
- `cargo test -p erpx-infrastructure --test support_persistence` — PASS: 8
  active tests, 8 documented database tests ignored.
- Disposable local PostgreSQL run:
  `support_ticket_list_is_bounded_ordered_and_status_filterable` — PASS: 1
  passed. Database `erpx_support_queue_20260725_01` was dropped afterward.
- Disposable local PostgreSQL run:
  `support_ticket_list_cursor_is_exclusive_deterministic_filtered_and_customer_scoped`
  — PASS: 1 passed. Database `erpx_support_cursor_20260725_01` was dropped
  afterward.
- Server cursor round-trip/security test — PASS.
- `cargo test --workspace` — PASS.

## Remaining work

Full support command/outbox atomicity, cursor/filter binding at the UI data
adapter, customer and platform UI data adapters, and the rest of G4 remain
open.
