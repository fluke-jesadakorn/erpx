# G4-002 support reply and internal-note transport evidence

## Scope

This packet adopts the existing authoritative support-ticket commands at the
Tonic boundary. `AddTicketReply` and `AddTicketInternalNote` require an
authenticated command context, a matching scoped support permission, an
existing ticket, and the entitled support module. Both commands use the
application command unit of work, so validation, optimistic concurrency,
outbox, audit, and idempotency remain on the write side.

Ticket responses contain customer-visible replies only. Internal notes remain
in the write aggregate and are excluded by the domain customer-visible
projection before serialization.

## Changed files

- `erpx/crates/proto/proto/erpx/platform/v1/platform.proto`
- `erpx/apps/server/src/main.rs`

## Verification

- `cargo fmt --package erpx-server --package erpx-proto` — PASS.
- `cargo test -p erpx-server server_security_tests::ticket_transport_excludes_internal_notes_from_customer_replies` — PASS.
- `cargo test -p erpx-server` — PASS; 51 tests.
- `cargo clippy -p erpx-server --all-targets -- -D warnings` — PASS.

## Residual work

Attachments, ticket list/search transport, SLA timers and notifications,
incident producers/broadcasts, and live/native support UI remain open. The
full G4 requirement is not closed by this packet.

## Security and rollback

Reply and internal-note permissions are distinct. No internal-note field is
present in the response DTO. Existing authentication, holding scope,
entitlement, and domain terminal-ticket checks remain authoritative. Rollback
is limited to the two changed source files and this evidence directory.

## Integration order

Integrate after the existing G4-002 domain/application/persistence packet and
before live support UI adapters.
