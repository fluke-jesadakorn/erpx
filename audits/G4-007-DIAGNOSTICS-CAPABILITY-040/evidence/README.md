# G4-007-DIAGNOSTICS-CAPABILITY-040

## Delivered behavior

- Added additive `SupportDiagnosticsAccess` protobuf context to
  `GetSupportDiagnosticsRequest`.
- Preserved the legacy platform-only request shape: an omitted access context
  is still operational platform diagnostics, but it requires platform scope
  and the authoritative `diagnostics:read` role.
- Added explicit platform mode. It rejects all customer/capability fields and
  requires platform scope and the same platform role.
- Added customer-support mode. It fails closed unless the request contains the
  customer, capability ID and bearer, support session and ticket IDs, the
  canonical approved scope, diagnostic-read mode, module, subscription, and
  canonical device reference.
- Customer-support diagnostics recheck, at request time:
  - authenticated access-token/session validity;
  - the active authenticated session device;
  - hash-only capability persistence, secret, status, expiry, ticket, session,
    agent, mode, permission, scope, and module bindings;
  - customer scope binding and approved-scope containment;
  - active support session and diagnostic-only domain policy;
  - active platform support-agent role and parent chain;
  - module-specific SoD, billing state, purchased entitlement, and
    company/branch allocation;
  - the domain support-intersection evaluator, including capability revocation
    and exact time boundaries.
- Customer-support reads record the customer ID in the existing audit event;
  platform operational reads retain no holding customer ID.
- Existing allow-list diagnostics construction and redaction remains the only
  payload path. No capability bearer, credentials, tokens, payment data, or
  arbitrary holding data is added to the response.

## Files changed

- `crates/proto/proto/erpx/platform/v1/platform.proto`
- `apps/server/src/main.rs`
- `audits/G4-007-DIAGNOSTICS-CAPABILITY-040/evidence/README.md`

## Contract and migration changes

- Protocol change is additive only: one message and one optional request field.
  Existing field numbers and platform response fields are unchanged.
- No database migration was required. Existing support capability persistence
  and hash-only authorization are consumed as-is.
- No worker, UI, Operations, identity, billing, or Folio files were changed.

## Verification

All commands were run from `erpx/`:

- `rustfmt --edition 2024 --check apps/server/src/main.rs` — PASS.
- `cargo test -p erpx-server` — PASS, 48 passed, 0 failed.
- `cargo clippy -p erpx-server --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-infrastructure --lib` — PASS, 43 passed, 0 failed, 3
  ignored (the ignored tests require disposable PostgreSQL).
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.

The server tests cover missing capability material, explicit platform-only
isolation, diagnostic-only mode, approved-scope containment, malformed device
and session bindings, expiry, revocation, immutable device/session/mode/scope/
module binding, and redaction. Existing infrastructure capability persistence
tests continue to cover hash-only storage and persisted revocation behavior.

The final workspace rerun is currently blocked before the server crate builds
by an unrelated existing compile error in
`crates/infrastructure/src/commercial.rs:963`: a publication-plan error derives
`Clone` while its `StripeCatalogRequestError` member does not implement `Clone`.
Resolving that requires a forbidden billing-file change and was not included in
this packet. The PASS results above were captured immediately before that
unrelated workspace error surfaced.

`cargo fmt --all --check` was also attempted. It is blocked by an unrelated
pre-existing formatter diff at `apps/worker/src/lib.rs:1346`; that shared worker
path is outside this packet's ownership and was not modified.

## Unfinished work and residual gaps

- This packet does not create live holding diagnostic producers, projection-lag
  collectors, incident projections, broadcasts, notifications, or the full
  support workflow/UI.
- The current response remains the existing bounded operational snapshot; it
  does not expose arbitrary per-holding business records.
- End-to-end gRPC/database tests for the complete handler path require a
  disposable control plane with identity, role, customer, billing, entitlement,
  support session, and capability fixtures. The unit boundary and existing
  persistence suites are the evidence delivered in this packet.
- G4 and G7 remain partial; this packet does not claim completion of either
  workstream or the ERPX migration.

## Security concerns

No known regression was introduced. The raw capability bearer exists only in
request-local memory, is never logged or audited, and is checked against the
persisted hash. Any missing, stale, revoked, mismatched, out-of-scope,
non-diagnostic, non-entitled, non-SOD, inactive-device, inactive-session, or
non-authorized request is rejected before snapshot generation.

## Integration order and rollback

Integrate after the existing support capability persistence, authenticated
device-bound support approval, and diagnostics redaction packets. The packet is
reverted by reverting only the additive protobuf field, the diagnostics handler
and its server tests, and this evidence directory; all other ERPX and Folio
changes remain untouched.
