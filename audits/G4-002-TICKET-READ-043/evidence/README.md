# G4-002 ticket detail read transport evidence

## Delivered behavior

Added the additive `SupportService.GetTicket` gRPC query. It requires an
authenticated session, a typed query context, the `support.ticket.read`
permission, customer-scope containment, and a non-empty audit purpose. The
server reads the authoritative control-plane ticket and records a
customer-attributed `support.ticket.read` audit event before returning only the
safe ticket DTO fields (ID, customer, status, severity, and description).

The query does not expose capability secrets, internal notes, raw payloads, or
holding credentials. A missing ticket is not converted into an empty success;
cross-scope access fails closed.

## Contract changes

- Added `GetTicketRequest` and `GetTicketResponse` to the platform protobuf.
- Added `SupportService.GetTicket`.
- No database or migration changes.

## Verification

- `cargo fmt -p erpx-server -- --check` — PASS.
- `cargo test -p erpx-server` — PASS; 50 tests.
- `cargo clippy -p erpx-server --all-targets -- -D warnings` — PASS.
- `cargo check -p erpx-proto` — PASS.

## Residuals

Ticket replies, internal notes, attachments, SLA scheduling, list pagination,
recurrence transport, customer/browser/native support surfaces, and worker
notifications remain open. This packet is read-only transport and does not
close the G4 gate.
