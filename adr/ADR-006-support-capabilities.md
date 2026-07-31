# ADR-006: Customer-approved support capabilities

**Status:** Accepted  
**Decision:** Support access is an immutable, random 256-bit opaque capability whose hash is persisted. It is bound to ticket, customer, agent, device/session, represented user, mode, permissions, modules, company/branch scope, reason, and expiry. Modes are capped at 8h for diagnostic read and 2h for mutation/impersonation. Approval requires an active Holding Owner or Customer Security Head.

**Rationale:** A ticket or agent role alone must not grant tenant access. Reissuing never extends an approval, and the effective authority is the intersection of all current constraints.

**Consequences:** Capability termination is triggered by expiry, revocation, closure, role/session expiry/revocation, parent expiry, or customer suspension. Impersonation attributes both platform and represented actors.

**Evidence required:** approval/scope/permission/module/agent/mode/expiry tests, revocation tests, ticket closure termination, represented-user authority tests, and customer-visible action audit.
