# G4-001 ticket recurrence evidence

## Delivered

- Closed support tickets remain terminal and cannot transition back to active states.
- A later recurrence creates a new `new` ticket with `recurrence_of` set to the closed predecessor.
- The authoritative application service emits recurrence creation through the existing command/outbox unit of work.
- Existing serialized tickets remain readable because the new link is serde-defaulted.

## Verification

- `cargo fmt -p erpx-domain -p erpx-application -p erpx-infrastructure -- --check` — PASS.
- `cargo test -p erpx-domain -p erpx-application` — PASS; 57 domain tests and 6 application tests.
- `cargo clippy -p erpx-domain -p erpx-application -p erpx-infrastructure --all-targets -- -D warnings` — PASS.

## Residual scope

The protobuf/server recurrence command, ticket replies/internal notes/attachments,
SLA and escalation workflows, read projections, and rendered/native support UI
are not claimed by this packet.

## Rollback

Revert the recurrence field/method, application command seam, fixture updates,
and this evidence file. No database or Folio state was changed.
