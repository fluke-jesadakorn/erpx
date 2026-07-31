# G10-MKT-024 — CRM command transport

## Contract

- **Owner:** main agent
- **Dependencies:** G10-MKT-019 domain handoff/attribution contract,
  G10-MKT-020 tenant write persistence, G10-MKT-021 read projection, and the
  shared authenticated command boundary.
- **Allowed paths:** CRM protobuf/server transport and focused transport tests,
  this audit, and plan/ledger records.
- **Forbidden paths:** CRM persistence internals, provider SDKs, Marketing
  delivery, Finance, Operations, UI, Folio, and command-owner/cutover state.

## Delivered behavior

- Added Tonic/gRPC-Web commands for saving an accepted-lead handoff, saving a
  CRM opportunity/lifecycle transition, and recording an attribution result.
- Commands carry bounded JSON representations of the existing validated domain
  records; the server does not reconstruct or accept caller-supplied revenue
  totals.
- Handoff and opportunity command IDs must match the authoritative transport
  command context and their scopes must be contained by the command scope.
- Every mutation requires authenticated actor binding, explicit
  holding/company/branch scope, active customer scope binding, the `crm.write`
  role, CRM entitlement, and routed tenant write access.
- Persistence remains authoritative for immutable handoffs, optimistic
  opportunity transitions, attribution recomputation, idempotency, outbox, and
  replay behavior. Audit records are written after successful mutations.

## Evidence

- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-server` — PASS.
- `cargo clippy -p erpx-server --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-server -- --nocapture` — 64 PASS.
- `cargo test -p erpx-proto -- --nocapture` — PASS.
- Existing disposable G10-MKT-020 CRM write test remains the evidence for
  atomic handoff/opportunity/attribution persistence and exact replay.

## Remaining boundary

Typed field-level CRM protobufs, CRM UI, profiles and consent, campaign and
journey execution, provider channels, Finance attribution reconciliation, and
the remaining G10 gate remain open. This packet does not claim G10 or ERPX
completion.

## Rollback

Remove the additive CRM command methods and server handlers while retaining the
existing write adapter. No tenant rows, Folio state, or external provider
state are changed by rollback.
