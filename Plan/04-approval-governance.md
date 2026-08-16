# Approval Governance Requirements

## Purpose

Approval governance is a shared ERPX capability. Source contexts continue to
own their business records, while Operations coordinates approval sessions,
gates, evidence, decisions, deadlines, delegations, and escalation.

### REQ-APR-001 — Version-pinned approval sessions

**Requirement.** ERPX shall create an approval session against immutable source
record versions and the policy version effective when the session begins.

**Rationale.** A decision must remain explainable even when the source record or
policy later changes.

**Acceptance criteria.** A session records source type, source ID, source
version, policy ID, and policy version; later policy publication affects only
new sessions; a changed source version invalidates or supersedes the old
session instead of silently reusing its decisions.

**Sources.** [Approval design](../audits/APPROVAL-GOVERNANCE-PENCIL-2026-08-13/README.md),
[Operations context](../crates/contexts/operations/src/lib.rs).

### REQ-APR-002 — Governed policy lifecycle

**Requirement.** ERPX shall manage approval policies through Draft, Review
Requested, Changes Requested, Approved, Scheduled, Active, Superseded, and
Archived states.

**Rationale.** Approval rules themselves require controlled review and
effective dating.

**Acceptance criteria.** Only an approved policy can be scheduled; activation
occurs at its effective time; only one applicable version is active for a
scope and condition set; superseded versions remain readable for audit; invalid
state transitions are rejected.

**Sources.** [Approval design](../audits/APPROVAL-GOVERNANCE-PENCIL-2026-08-13/README.md),
[policy design evidence](../audits/APPROVAL-GOVERNANCE-PENCIL-2026-08-13/evidence/system/policies.png).

### REQ-APR-003 — Gates and required evidence

**Requirement.** ERPX shall express each approval policy as ordered or parallel
gates with explicit eligibility, evidence, decision, and completion rules.

**Rationale.** Approvers need to know what must be proven before a record may
advance.

**Acceptance criteria.** Each gate identifies its approver rule and required
evidence; unavailable or restricted evidence is visible as such and cannot be
treated as reviewed; parallel gates complete only under their configured
decision rule; incomplete required gates prevent release.

**Sources.** [Approval design](../audits/APPROVAL-GOVERNANCE-PENCIL-2026-08-13/README.md),
[gate detail evidence](../audits/APPROVAL-GOVERNANCE-PENCIL-2026-08-13/evidence/operational/gate-detail.png).

### REQ-APR-004 — Independent decision authority

**Requirement.** ERPX shall enforce maker-checker separation, exact-scope
authority, approval limits, risk tier, and fresh authentication at decision
time.

**Rationale.** Eligibility at session creation is insufficient if authority or
risk changes before the decision.

**Acceptance criteria.** The requester cannot approve when separation is
required; an expired or revoked assignment cannot decide; amount and scope
limits are evaluated against current authority; high-risk decisions require
fresh backend-verified MFA.

**Sources.** [Universal RBAC](../adr/ADR-004-expiring-rbac.md),
[approval design](../audits/APPROVAL-GOVERNANCE-PENCIL-2026-08-13/README.md).

### REQ-APR-005 — Complete decision outcomes

**Requirement.** ERPX shall support approve, reject, request changes, and other
policy-defined outcomes without converting one outcome into another.

**Rationale.** A request for correction is not a rejection, and an expired gate
is not an approval.

**Acceptance criteria.** Every decision records actor, represented actor when
applicable, timestamp, reason, evidence version, and resulting gate state;
request-changes returns the source record to its defined correction path;
rejection and expiry produce distinct terminal or recovery outcomes.

**Sources.** [Decision outcomes](../audits/APPROVAL-GOVERNANCE-PENCIL-2026-08-13/evidence/operational/decision-outcomes.png),
[Operations context](../crates/contexts/operations/src/lib.rs).

### REQ-APR-006 — Delegation without authority expansion

**Requirement.** ERPX shall permit time-bounded approval delegation only when
the delegate independently satisfies the gate's current role, scope, risk, and
limit requirements.

**Rationale.** Delegation should maintain continuity without creating broader
authority.

**Acceptance criteria.** Delegation records source assignment, delegate,
scope, reason, validity, and creator; it cannot outlive the delegator's
assignment; revocation or expiry removes eligibility immediately; the UI shows
the delegation banner on affected work.

**Sources.** [Approval design](../audits/APPROVAL-GOVERNANCE-PENCIL-2026-08-13/README.md),
[delegation evidence](../audits/APPROVAL-GOVERNANCE-PENCIL-2026-08-13/evidence/system/delegations.png).

### REQ-APR-007 — Deadlines and escalation

**Requirement.** ERPX shall calculate gate deadlines from the pinned policy and
shall notify and escalate overdue work according to the policy's ordered steps.

**Rationale.** Time-sensitive business work needs predictable handling rather
than hidden queue aging.

**Acceptance criteria.** Due and escalation times are stored with the session;
workers can resume without duplicating notifications; escalation cannot grant
authority to an ineligible actor; expired work produces an explicit outcome
and audit event.

**Sources.** [Approval design](../audits/APPROVAL-GOVERNANCE-PENCIL-2026-08-13/README.md),
[escalation evidence](../audits/APPROVAL-GOVERNANCE-PENCIL-2026-08-13/evidence/system/escalations.png),
[Operations application](../crates/contexts/operations/src/application/mod.rs).

### REQ-APR-008 — Policy simulation and immutable audit

**Requirement.** ERPX shall let authorized administrators simulate a draft
policy against bounded test facts before publication and shall retain an
immutable history of policy and session decisions.

**Rationale.** Administrators need to detect unsafe gaps without changing live
authority, and auditors need reproducible decision history.

**Acceptance criteria.** Simulation has no business side effect and identifies
matched policy, gates, eligible roles, and unresolved conditions; publication
requires the configured review checklist; audit shows policy versions,
session transitions, decisions, delegation, escalation, and invalidation.

**Sources.** [Approval design](../audits/APPROVAL-GOVERNANCE-PENCIL-2026-08-13/README.md),
[simulation evidence](../audits/APPROVAL-GOVERNANCE-PENCIL-2026-08-13/evidence/system/policy-simulation.png),
[audit evidence](../audits/APPROVAL-GOVERNANCE-PENCIL-2026-08-13/evidence/system/audit.png).
