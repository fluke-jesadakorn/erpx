# G2-IAM-004 — Recursive parent-chain authorization

## Objective

Ensure universal write-side access decisions recheck every role-assignment
ancestor rather than only the immediate parent.

## Dependencies and ownership

- Owner: main agent; shared domain and server integration.
- Depends on G2-IAM-003 principal and allocation intersection.
- Consumes authoritative role-assignment rows/snapshots from the control write
  database.
- No migration or external database state changes.

## Delivered behavior

- `evaluate_effective_access_chain` validates the immediate-to-root chain at
  the same instant as the child assignment.
- Each edge checks parent identity, active half-open window, scope
  containment, authority limit, and permission containment.
- Missing ancestors and cyclic chains fail closed.
- SQLx exposes a validated chain in immediate-to-root order.
- The universal server access, entitlement-check, and object-access paths use
  the complete chain.
- Existing immediate-parent repository callers remain source-compatible.

## Evidence

- `cargo test -p erpx-domain access::tests` — 19 passed.
- Chain tests cover a valid two-level chain, missing ancestor, and ancestor
  expiry boundary.
- `cargo check -p erpx-server` — PASS.

## Nonclaims

This packet does not claim recursive parent-chain transport for the separate
support-intersection DTO, complete universal mutation adoption, durable session
resolution in every caller, approver identity resolution, role-review worker
delivery, recovery replay persistence, or completion of G2-G14.
