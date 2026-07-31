# G4-001 support ticket transport evidence

## Delivered behavior

`SupportService.UpdateTicket` now authenticates the actor, validates the
command context and support entitlement, applies the authoritative domain
lifecycle transition, persists the versioned command/outbox event, and returns
the updated ticket. Closed is parsed as an explicit terminal status. A closed
ticket also invokes the bounded ticket capability revocation seam.

## Independent verification

| Command | Result |
|---|---|
| `cargo test -p erpx-server` | PASS; 42 tests |
| `cargo fmt --all --check` | PASS |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS |

## Remaining boundary

The full intake, reply, attachment, SLA, escalation, incident-linking,
satisfaction, and customer/native route surfaces remain open. Terminal-ticket
capability revocation follows the authoritative ticket command in a bounded
follow-up drain and is not yet one atomic transaction with that command/outbox
write.

## Rollback

Remove the additive `UpdateTicket` protobuf/RPC mapping, application command,
server handler, and ticket revocation call. Folio remains the write owner.
