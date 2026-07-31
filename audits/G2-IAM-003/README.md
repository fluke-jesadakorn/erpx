# G2-IAM-003 — Universal effective-access intersection

## Objective

Make the shared write-side authorization evaluator enforce principal binding,
expiring RBAC, scope containment, commercial state, and company/branch module
activation as one fail-closed intersection.

## Dependencies and ownership

- Owner: main agent; shared domain and server integration.
- Consumes: `RoleAssignment`, `Scope`, billing lifecycle, and
  `EntitlementAllocationBook`.
- Allowed paths: `crates/domain/src/access.rs` and the three existing server
  authorization call sites in `apps/server/src/main.rs`.
- No migration or external database state changes.

## Delivered behavior

- The request principal must equal the authoritative assignment principal.
- Company and branch requests require an allocation book.
- Company requests require an active allocated company module.
- Branch requests require an active allocated company module and explicit
  branch activation.
- Purchased entitlement values come from the supplied allocation book when it
  is present, preventing a caller from replacing them with client data.
- Existing exact half-open role boundaries, parent containment, approval
  limits, billing state, support capability, and SoD checks remain enforced.
- The server passes the control-plane allocation book into all current
  effective-access paths.

## Evidence

- `cargo test -p erpx-domain access::tests` — 18 passed.
- Principal mismatch, missing allocation, company activation, branch
  activation, parent cascade, half-open expiry, billing, approval, and
  support-capability cases are covered by unit tests.

## Nonclaims

This packet does not claim complete universal mutation adoption, durable
session revocation lookup in every caller, recursive parent-chain loading,
authoritative approver resolution, support-capability retrieval, worker role
expiry delivery, recovery replay persistence, or completion of G2-G14.
