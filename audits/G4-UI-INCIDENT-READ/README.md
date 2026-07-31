# G4 authenticated platform incident and support-ticket read surfaces

Date: 2026-07-27  
Status: local read-surface continuation; G4 remains open.

## Scope

The Dioxus console now consumes the existing `SupportService.ListIncidents`
and `SupportService.ListTickets` RPCs for the `/platform/incidents` and
`/platform/support` routes. Both adapters create a platform-scoped query
context with actor/session attribution, send authenticated transport metadata,
bound pages to 100 records, validate correlation and pagination references, and
convert only shared allow-listed DTO fields into redaction-safe display models.

Incident timeline, broadcast, mitigation, root-cause, corrective-action,
evidence, capability, credential, and command fields do not cross into the
console model. Invalid records fail closed. The UI distinguishes loading,
empty, denied, offline, error, and read-only states; incident commands remain
server-authorized and are not exposed by this surface.

The support-ticket adapter exposes only opaque ticket/customer references,
typed lifecycle status, and bounded severity. Ticket descriptions, customer
replies, internal notes, capability material, and commands do not cross into
the console model. Invalid rows and inconsistent pagination fail closed.

## Verification

- PASS: `cargo fmt --all`.
- PASS: `cargo test -p erpx-ui --offline` — 80 passed, 0 failed.
- PASS: `cargo test -p erpx-console --offline` — 73 passed, 0 failed.
- PASS: focused wire-boundary coverage rejects invalid incident/ticket rows,
  inconsistent ticket cursors, and keeps display models free of internal
  support material.

This packet is local evidence for the incident list adapter only. It does not
claim full G4 support/incident workflow completion, durable notification
delivery, approval containment, customer/tenant support UI, device execution,
live reconciliation, or G4/G14 closure.

No production directory activation, command-owner change, Folio write
retirement, or cutover occurred.
