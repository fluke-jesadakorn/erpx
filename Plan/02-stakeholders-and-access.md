# Stakeholder and Access Requirements

## Stakeholders

ERPX serves platform staff, customer administrators, business operators,
approvers, auditors, support personnel, and external customer users. A person
may hold several assignments but acts through one explicit active role and
scope at a time.

### REQ-IAM-001 — Platform Root boundary

**Requirement.** ERPX shall reserve Platform Root for platform bootstrap and
governed platform operations and shall not treat Root as standing authority to
read or mutate customer business data.

**Rationale.** Infrastructure authority must not silently become customer
business authority.

**Acceptance criteria.** Root actions display the Root actor identity; customer
business access requires a separate, current customer-approved capability;
customer-hosted agents can reject Root access; protected break-glass
principals cannot be replaced through directory collision resolution.

**Sources.** [Support capabilities](../adr/ADR-006-support-capabilities.md),
[branch and directory authority](../adr/ADR-013-holding-branch-database-and-directory-authority.md),
[identity implementation](../crates/identity/src/lib.rs).

### REQ-IAM-002 — Holding Owner authority

**Requirement.** ERPX shall allow a Holding Owner to govern the holding,
companies, branches, business-unit availability, directory configuration, and
customer approvals within that holding.

**Rationale.** Customers need sovereign administration without routine Platform
Root intervention.

**Acceptance criteria.** A Holding Owner can administer only the assigned
holding; high-risk operations require fresh MFA and, where specified, a second
eligible approver; platform-only policy and non-brandable safety controls
remain outside Holding Owner authority.

**Sources.** [Branch authority](../adr/ADR-013-holding-branch-database-and-directory-authority.md),
[corporate identity](../adr/ADR-012-three-layer-corporate-identity.md),
[identity-access context](../crates/contexts/identity-access/src/lib.rs).

### REQ-IAM-003 — Company and Branch administration

**Requirement.** ERPX shall allow Company and Branch administrators to manage
delegated organization, people, configuration, and business work only within
their assigned scope.

**Rationale.** Operational control should sit near the work while preserving
holding-wide policy and separation.

**Acceptance criteria.** Child assignments cannot exceed their parent scope or
permissions; branch administrators cannot provision an unrelated database or
directory authority; cross-branch actions are denied unless an explicit
parent-scope role authorizes them.

**Sources.** [Universal RBAC](../adr/ADR-004-expiring-rbac.md),
[organization context](../crates/contexts/organization/src/lib.rs),
[branch authority](../adr/ADR-013-holding-branch-database-and-directory-authority.md).

### REQ-IAM-004 — Business workspace roles

**Requirement.** ERPX shall define independently entitled Viewer, Operator,
Approver, Administrator, and Auditor responsibilities for each canonical
business workspace.

**Rationale.** Access should reflect business duty rather than a broad,
all-module administrator role.

**Acceptance criteria.** Role discovery uses the canonical workspace catalog;
the active role determines visible actions but never replaces server-side
authorization; a missing entitlement or permission denies the command even if
the route is visible.

**Sources.** [Business-unit catalog](../crates/domain/src/data_plane.rs),
[navigation access](../crates/ui/src/navigation.rs),
[identity-access context](../crates/contexts/identity-access/src/lib.rs).

### REQ-IAM-005 — Expiring role assignments

**Requirement.** ERPX shall make every role assignment time-bounded,
scope-bounded, versioned, attributable, and non-renewable in place.

**Rationale.** Expiry and reassessment are security properties, not optional
administrative reminders.

**Acceptance criteria.** No command can create a permanent assignment;
validity uses the half-open interval `[valid_from, valid_until)`; renewal
creates a new immutable version; evaluation at or after `valid_until` denies
access.

**Sources.** [Universal RBAC](../adr/ADR-004-expiring-rbac.md),
[access domain](../crates/domain/src/access.rs),
[role lifecycle API](../crates/proto/proto/erpx/identity_access/v2/service.proto).

### REQ-IAM-006 — Parent containment and revocation

**Requirement.** ERPX shall prevent a delegated assignment from outliving or
exceeding its parent and shall cascade effective revocation through dependent
assignments and active work.

**Rationale.** Delegation is safe only while the delegating authority remains
valid.

**Acceptance criteria.** Child scope, permissions, risk, and validity are
contained by the parent; parent expiry or revocation removes child effective
access; affected work is reassigned or surfaced for review; immutable history
is retained.

**Sources.** [Universal RBAC](../adr/ADR-004-expiring-rbac.md),
[role review worker](../apps/worker/src/role_review_runtime.rs),
[role review tests](../apps/worker/tests/role_review_notifications.rs).

### REQ-IAM-007 — High-risk separation of duties

**Requirement.** ERPX shall require fresh authentication and independent
maker-checker approval for designated high-risk access, identity, financial,
support, recovery, and policy operations.

**Rationale.** A single compromised or conflicted actor must not complete a
high-impact action alone.

**Acceptance criteria.** The requester cannot satisfy the independent approval;
both actors are currently authorized at the exact scope; both authentication
assertions are fresh; stale, expired, rejected, or self-approved evidence
cannot execute the action.

**Sources.** [Universal RBAC](../adr/ADR-004-expiring-rbac.md),
[support capabilities](../adr/ADR-006-support-capabilities.md),
[approval design](../audits/APPROVAL-GOVERNANCE-PENCIL-2026-08-13/README.md).

### REQ-IAM-008 — Directory and local identities

**Requirement.** ERPX shall support governed local identities and immutable
directory-managed identities without merging people solely by username or
email.

**Rationale.** Mutable human-readable attributes are unsafe identity keys.

**Acceptance criteria.** Directory identities use provider authority and an
immutable provider identifier; ambiguous local collisions create a review
record; approved replacement revokes all credentials on the former principal
without transferring roles; historical actions retain the original principal.

**Sources.** [Branch and directory authority](../adr/ADR-013-holding-branch-database-and-directory-authority.md),
[directory contracts](../crates/proto/proto/erpx/identity_access/v2/service.proto),
[identity synchronization](../crates/identity/src/directory/sync.rs).

### REQ-IAM-009 — Explicit active context

**Requirement.** ERPX shall make the active account, role, tenant, holding,
company, branch, and support or impersonation context visible and changeable
only through an authorized context transition.

**Rationale.** Users must understand which authority will be applied before
they act.

**Acceptance criteria.** Protected pages display the active context; switching
role or scope re-evaluates navigation and commands; ending an administrative
or support context removes its authority immediately; context identifiers in
URLs and diagnostics are bounded and non-secret.

**Sources.** [Console composition](../apps/console/src/composition.rs),
[platform administration API](../crates/proto/proto/erpx/platform_administration/v2/service.proto),
[UI navigation](../crates/ui/src/navigation.rs).

## Complete identity-governance lifecycle

| ID | Title | Normative requirement | Rationale | Acceptance paths | Source | Scenarios |
|---|---|---|---|---|---|---|
| REQ-IAM-010 | Principal lifecycle | ERPX shall govern create, invite, activate, suspend, disable, reactivate, and retire for every principal. | Access needs explicit state. | Success: valid transition is attributable; Denial: disabled principal cannot authenticate; Failure: partial downstream revocation remains blocking work. | [FSD-002](decisions/FSD-002-shared-platform-capabilities.md) | E2E-26 |
| REQ-IAM-011 | Role definition lifecycle | ERPX shall version role purpose, permissions, scope ceiling, owner, approval, activation, retirement, and supersession. | Role meaning must not drift. | Success: assignment pins role version; Denial: draft/retired role cannot be newly assigned; Failure: incompatible change creates a new version. | [FSD-002](decisions/FSD-002-shared-platform-capabilities.md) | E2E-26 |
| REQ-IAM-012 | Protected identities | ERPX shall protect seed, Root, emergency, provider, and agent identities from unsafe deletion, self-escalation, or unmanaged ownership. | Loss or abuse of special identities is high risk. | Success: protected change uses independent approval; Denial: self-grant/self-delete is rejected; Failure: owner loss triggers recovery procedure. | [ADR-004](../adr/ADR-004-expiring-rbac.md) | E2E-01, E2E-26 |
| REQ-IAM-013 | Joiner mover leaver | ERPX shall derive joiner, mover, and leaver access actions from accepted People and directory facts. | Workforce changes must propagate safely. | Success: eligible access activates/expires; Denial: unverified HR event grants nothing; Failure: orphaned access enters remediation. | [FSD-002](decisions/FSD-002-shared-platform-capabilities.md) | E2E-26 |
| REQ-IAM-014 | Access request | ERPX shall govern access request, justification, owner approval, SoD evaluation, time limit, activation, denial, and expiry. | Ad hoc access still needs policy. | Success: approved request creates bounded assignment; Denial: conflict/unapproved request grants nothing; Failure: activation error is reconciled. | [FSD-002](decisions/FSD-002-shared-platform-capabilities.md) | E2E-26 |
| REQ-IAM-015 | Access certification | ERPX shall run scoped certification of human, agent, service, role, and group access with owner decisions and evidence. | Long-lived access needs review. | Success: reviewer certifies/revokes each item; Denial: conflicted reviewer cannot certify; Failure: overdue review follows policy. | [FSD-002](decisions/FSD-002-shared-platform-capabilities.md) | E2E-26 |
| REQ-IAM-016 | Break-glass | ERPX shall require bounded reason, fresh authentication, duration, monitoring, termination, and independent after-review for break-glass access. | Emergencies must not become permanent access. | Success: eligible session expires and is reviewed; Denial: missing condition blocks activation; Failure: monitoring loss terminates or fails closed. | [FSD-002](decisions/FSD-002-shared-platform-capabilities.md) | E2E-26 |
| REQ-IAM-017 | Non-human identity | ERPX shall govern owner, purpose, credential, scopes, rotation, workload binding, suspension, and retirement for service and agent principals. | Machines require accountable identities. | Success: approved workload uses scoped credential reference; Denial: ownerless identity cannot run; Failure: rotation gap revokes access. | [FSD-005](decisions/FSD-005-collaboration-references-and-agents.md) | E2E-26, E2E-46 |
| REQ-IAM-018 | Orphan work | ERPX shall identify and reassign open work, approvals, threads, schedules, and agent ownership before principal retirement. | Deactivation must not strand obligations. | Success: accountable successor accepts work; Denial: retirement cannot silently discard it; Failure: unresolved work blocks final retirement or enters governed queue. | [FSD-002](decisions/FSD-002-shared-platform-capabilities.md) | E2E-26 |
| REQ-IAM-019 | Separate realms | ERPX shall keep workforce/admin, commerce/customer/guest, candidate, one-time signatory, agent, and service realms distinct with explicit linking. | Shared email or phone is not authority. | Success: verified link preserves separate credentials/roles; Denial: automatic merge/role transfer is rejected; Failure: conflict requires resolution. | [FSD-002](decisions/FSD-002-shared-platform-capabilities.md) | E2E-26, E2E-36, E2E-37, E2E-58 |
