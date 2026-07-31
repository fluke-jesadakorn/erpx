# G2-IAM-006 — Authoritative approver identity resolution

## Objective

Prevent transport callers from naming arbitrary approvers for high-risk role
renewal and recovery commands.

## Dependencies and ownership

- Owner: main agent; identity and server authorization integration.
- Depends on active token/session authorization, control-plane role-chain
  lookup, and G2-IAM-004 chain evidence.
- No migration or external state change.

## Delivered behavior

- The identity store exposes a bounded active-principal lookup for named
  second-party approvers.
- Role renewal validates a supplied approver against active identity and
  `role.assignment.approve` in the current assignment scope.
- Final-role recovery validates its approver the same way and validates an
  optional reviewer against `role.assignment.review`.
- Empty, self, inactive, missing-role, and out-of-scope approvers fail before
  the authoritative role command is executed.

## Evidence

- `cargo test -p erpx-identity` — 20 passed; six disposable SQLx tests remain
  explicitly ignored.
- `cargo check -p erpx-server` — PASS.
- Active token/session and role-chain lookups remain write-side boundaries.

## Nonclaims

This packet does not claim a separate interactive approval artifact, complete
universal mutation adoption, notification recipient/provider policy, recovery
replay persistence, or completion of G2-G14.
