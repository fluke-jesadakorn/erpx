# G5-BIL-049 — Entitlement command runtime

Date: 2026-07-30

## Scope

The commercial entitlement aggregate already enforced capacity, allocation,
company activation, branch-limit, billing-lifecycle, and revocation rules.
This packet adopted those transitions as authoritative customer-facing
commands and connected the ERPX console to the same server-owned boundary.

## Authoritative path

- `AllocateCompanyEntitlement`, `ActivateCompanyEntitlement`, and
  `ActivateBranchEntitlement` are additive protobuf RPCs on
  `EntitlementService`.
- The server requires an authenticated actor match, explicit holding/company/
  branch containment, customer scope, the commercial permission, a live
  subscription lifecycle, an entitled module, effective role-chain access,
  active segregation-of-duties state, and a fresh one-time MFA step-up. The
  MFA code is transport-only and never enters command context, audit, outbox,
  receipts, or errors. The target company/branch may be
  inactive while its administrative activation command is evaluated.
- Infrastructure command wrappers lock the entitlement aggregate, apply an
  optional expected aggregate version, reserve the commercial idempotency key,
  persist the new snapshot, append the typed state event/outbox record and
  append-only audit record in the same SQLx transaction, then return a durable
  receipt. Replays return the original receipt; stale versions fail without a
  state change.
- The console exposes a scoped entitlement read/evaluation and forms for
  allocation plus company/branch activation. It sends the real RPCs and
  renders only server receipts or bounded error state.

## Evidence

- Infrastructure commercial active tests: 29/29.
- Fresh disposable PostgreSQL command acceptance:
  `entitlement_allocation_and_activation_commands_are_atomic_versioned_and_replay_safe` 1/1.
- Server tests: 94/94; server and infrastructure warnings-denied Clippy passed.
- Console desktop check passed after the UI wiring; the complete target matrix
  is rerun in the coordinator verification below.

## Rollback

The packet is additive and has no migration. Rollback is a source/protobuf
revert that removes the three RPC registrations and console entry point;
existing provider entitlement persistence remains unchanged. No customer
directory was activated and no production command-owner or Folio write path
was changed.

## Open acceptance

Production Stripe/catalog reconciliation, external directory certification,
approved protobuf provenance, physical recovery/device/scale evidence, and
production approval remain queued. This packet does not authorize activation
or cutover.
