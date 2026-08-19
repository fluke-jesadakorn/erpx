# ADR-006: Customer-approved support capabilities

**Status:** Accepted  
**Decision:** Support access is an immutable, random 256-bit opaque capability whose hash is persisted. It is bound to ticket, customer, agent, device/session, represented user, mode, permissions, modules, company/branch scope, reason, and expiry. Modes are capped at 8h for diagnostic read and 2h for mutation/impersonation. Approval requires an active Holding Owner or Customer Security Head.

For a customer-hosted data plane, every interactive ERPX Root session additionally requires a new customer approval with fresh MFA. The signed one-time capability is delivered through the customer's outbound mTLS agent channel and is bound to the Root principal, authenticated device, holding, agent, exact scope, business units, operations, and TTL. The agent is the final admission point. No platform policy, emergency label, or standing Root role can bypass rejection, missing approval, expiry, revocation, or agent disconnection.

**Rationale:** A ticket or agent role alone must not grant holding access. Reissuing never extends an approval, and the effective authority is the intersection of all current constraints.

**Consequences:** Capability termination is triggered by expiry, revocation, closure, role/session expiry/revocation, parent expiry, customer suspension, or customer-agent disconnection. Impersonation attributes both platform and represented actors. Infrastructure authority cannot read business data; represented-user support access remains a separate capability.

**Evidence required:** approval/scope/permission/module/agent/device/mode/expiry tests, requester/approver separation and fresh-MFA tests, rejection and offline-agent denial, revocation tests, ticket closure termination, represented-user authority tests, and customer-visible action audit.
