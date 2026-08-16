# Customer and Support Requirements

## Portfolio boundary

Customer Authority owns canonical customer identity and lifecycle. Customer
360 composes authorized read-only facts from customer, commercial, support,
organization, and tenant activity sources. Support owns tickets, incidents,
capabilities, and customer-approved support sessions.

## Customer Authority

### REQ-CUS-001 — Canonical customer account

**Requirement.** ERPX shall maintain one canonical customer account identity,
code, status, and version within its authoritative scope.

**Rationale.** Sales, service, billing, and access audit need a stable customer
reference without sharing ownership.

**Acceptance criteria.** Customer creation is idempotent; customer code and
identity rules are validated; updates require expected version; other contexts
refer to the customer by opaque ID and versioned facts rather than mutating it.

**Sources.** [Customer Authority context](../../crates/contexts/customer-authority/src/lib.rs),
[Customer API](../../crates/proto/proto/erpx/customer_authority/v1/service.proto).

### REQ-CUS-002 — Customer lifecycle and identity maintenance

**Requirement.** ERPX shall support governed customer status and identity
changes while preserving historical attribution and dependent references.

**Rationale.** Customer details evolve, but earlier business records must remain
explainable.

**Acceptance criteria.** Status changes follow valid transitions; identity
updates do not replace the customer ID; suspended or closed customers cannot
start disallowed new work; history records actor, reason, prior version, and
new version.

**Sources.** [Customer Authority context](../../crates/contexts/customer-authority/src/lib.rs),
[customer authority domain](../../crates/contexts/customer-authority/src/domain/mod.rs).

### REQ-CUS-003 — Governed customer merge

**Requirement.** ERPX shall merge duplicate customer authority only through an
explicit, version-checked command that identifies a surviving target and
preserves source history.

**Rationale.** Silent merging can corrupt financial, support, and audit
references.

**Acceptance criteria.** Source and target are in compatible scope and current
versions; the source becomes non-authoritative without deleting history;
integration facts identify the merge; conflicting or already-merged records
are rejected without side effects.

**Sources.** [Customer Authority context](../../crates/contexts/customer-authority/src/lib.rs),
[Customer API](../../crates/proto/proto/erpx/customer_authority/v1/service.proto).

### REQ-CUS-004 — External authority links

**Requirement.** ERPX shall link and unlink approved external customer
authorities without making an external display identifier the ERPX customer
primary key.

**Rationale.** Provider and partner identifiers change independently of the
customer's ERPX identity.

**Acceptance criteria.** Links identify authority, external key, scope, and
version; duplicate conflicting links are rejected; unlinking retains audit and
does not erase business records; unauthorized provider payload is not exposed.

**Sources.** [Customer Authority context](../../crates/contexts/customer-authority/src/lib.rs),
[customer integration contracts](../../crates/contexts/customer-authority/src/integration/mod.rs).

## Customer 360

### REQ-C360-001 — Composed customer view

**Requirement.** ERPX shall provide an authorized Customer 360 view composed
from customer overview, organization, contacts, commercial, entitlement,
onboarding, health, support, access-audit, notes, and relationship-activity
projections.

**Rationale.** Customer-facing teams need a coherent view without creating a
second mutation owner.

**Acceptance criteria.** Every section identifies source coverage and
availability; Customer 360 performs no source aggregate mutation; the view is
bound to one customer and authorized purpose; unavailable sections do not
block safe available sections.

**Sources.** [Customer 360 context](../../crates/contexts/customer-360/src/lib.rs),
[Customer 360 API](../../crates/proto/proto/erpx/customer_360/v1/service.proto),
[customer console routes](../../apps/console/src/composition.rs).

### REQ-C360-002 — Section freshness and completeness

**Requirement.** ERPX shall expose authoritative freshness, source position,
and completeness for each Customer 360 section.

**Rationale.** Users must know whether a composed view is current enough for a
decision.

**Acceptance criteria.** Section state distinguishes ready, stale,
not-projected, unavailable, and denied; a five-minute-old customer read is not
reported fresh when it exceeds the declared budget; row count is not used as a
freshness substitute.

**Sources.** [Customer 360 context](../../crates/contexts/customer-360/src/lib.rs),
[customer domain](../../crates/domain/src/customer.rs),
[product surface contracts](../../apps/console/src/product_completion.rs).

### REQ-C360-003 — Audited detailed access

**Requirement.** ERPX shall route platform detailed customer reads through an
audited gateway and shall make the resulting access history visible to an
authorized customer security role.

**Rationale.** Platform visibility must be transparent to the customer.

**Acceptance criteria.** Detail access records actor, represented actor when
applicable, customer, section, scope, purpose, time, correlation ID, and
outcome; list discovery and detail access have distinct audit semantics;
denied attempts are recorded without leaking protected values.

**Sources.** [Audited global reads](../../adr/ADR-005-global-read-auditing.md),
[Customer access-audit API](../../crates/proto/proto/erpx/customer_authority/v1/service.proto),
[customer access audit tests](../../apps/worker/tests/customer_access_audit.rs).

### REQ-C360-004 — Governed customer notes

**Requirement.** ERPX shall keep customer notes bounded, attributable,
scope-authorized, and separate from credentials and unrestricted business
documents.

**Rationale.** Notes are useful collaboration data but can become an
uncontrolled sensitive-data channel.

**Acceptance criteria.** Note creation records author, purpose, scope, and
timestamp; content limits and redaction apply; only authorized readers see the
note projection; deleted or superseded presentation retains required audit
history.

**Sources.** [Customer note API](../../crates/proto/proto/erpx/customer_360/v1/service.proto),
[Customer 360 context](../../crates/contexts/customer-360/src/lib.rs).

## Support and incidents

### REQ-SUP-001 — Support ticket lifecycle

**Requirement.** ERPX shall manage support tickets through explicit creation,
assignment, communication, SLA, resolution, closure, and recurrence behavior.

**Rationale.** Customer support work needs a durable, visible lifecycle.

**Acceptance criteria.** Ticket transitions are validated and versioned;
assignment and SLA are explicit; closing a ticket terminates its dependent
support access; recurrence creates a traceable relationship rather than
silently reopening unrelated history.

**Sources.** [Support context](../../crates/contexts/support/src/lib.rs),
[Support API](../../crates/proto/proto/erpx/support/v1/service.proto),
[support workflow tests](../../apps/worker/tests/support_workflow.rs).

### REQ-SUP-002 — Audience-controlled messages

**Requirement.** ERPX shall classify each support message by audience and shall
prevent internal or restricted content from appearing in a customer-visible
thread.

**Rationale.** A ticket can contain customer communication and internal
diagnosis with different disclosure rules.

**Acceptance criteria.** Message audience is immutable after publication;
queries enforce the reader's audience; internal content is not returned as a
redacted placeholder that confirms its substance; message authorship and time
remain auditable.

**Sources.** [Support context](../../crates/contexts/support/src/lib.rs),
[Support API](../../crates/proto/proto/erpx/support/v1/service.proto).

## Lifecycle-completion requirements

| ID | Title | Normative requirement | Rationale | Acceptance paths | Source | Scenarios |
|---|---|---|---|---|---|---|
| REQ-CUS-090 | Customer lifecycle coverage | ERPX shall govern create, verify, approve, activate, change, merge, suspend, close, erase, and archive for customer authority records. | Customer identity needs complete outcomes. | Success: valid transition preserves lineage; Denial: unsafe merge/erasure is rejected; Failure: unresolved external link blocks completion. | [FSD-003](../decisions/FSD-003-enterprise-suite-expansion.md) | E2E-04, E2E-31 |
| REQ-CUS-091 | Customer reconciliation | ERPX shall reconcile customer identity with CRM, Sales, Commerce, Support, Legal, Finance, and external links. | Multiple contexts consume customer facts. | Success: consumers converge to version; Denial: consumer cannot edit authority; Failure: mismatch enters governed resolution. | [FSD-003](../decisions/FSD-003-enterprise-suite-expansion.md) | E2E-04, E2E-36 |
| REQ-C360-090 | Customer-view lifecycle coverage | ERPX shall govern section registration, publication, freshness, restriction, withdrawal, rebuild, and archive for Customer 360 composition. | Composed views require lifecycle too. | Success: authorized section declares state; Denial: unavailable/restricted source is not fabricated; Failure: section outage is isolated. | [FSD-003](../decisions/FSD-003-enterprise-suite-expansion.md) | E2E-04, E2E-16 |
| REQ-C360-091 | Customer-view reconciliation | ERPX shall reconcile every Customer 360 section to source cursor, access audit, deletion, and customer-visible evidence. | Aggregation can drift. | Success: section reaches source position; Denial: hidden data/counts stay absent; Failure: gap triggers rebuild. | [FSD-003](../decisions/FSD-003-enterprise-suite-expansion.md) | E2E-04, E2E-68 |
| REQ-SUP-090 | Support lifecycle coverage | ERPX shall govern ticket, message, attachment, access request, capability, represented action, incident, confirmation, closure, and archive. | Support crosses customer boundaries. | Success: valid state records both actors where needed; Denial: capability-less access is rejected; Failure: revocation wins the next operation. | [FSD-003](../decisions/FSD-003-enterprise-suite-expansion.md) | E2E-04, E2E-19 |
| REQ-SUP-091 | Support reconciliation | ERPX shall reconcile customer approval, capability use, attachments, incident SLA, provider messages, revocation, and customer audit. | Customer trust needs complete evidence. | Success: customer view matches platform facts; Denial: support cannot hide access; Failure: evidence gap blocks closure. | [FSD-003](../decisions/FSD-003-enterprise-suite-expansion.md) | E2E-04, E2E-20 |

### REQ-SUP-003 — Protected attachments

**Requirement.** ERPX shall store support attachments in tenant-bound object
storage and grant only short-lived, authorized access for the ticket audience.

**Rationale.** Attachment URLs and objects can disclose customer data outside
the ticket boundary.

**Acceptance criteria.** Metadata records digest, size, media type, uploader,
audience, and ticket; objects use scoped keys; download requires current ticket
and object permission; credentials and permanent public URLs are never
returned.

**Sources.** [Support attachment worker](../../apps/worker/src/support_attachments.rs),
[support attachment tests](../../apps/worker/tests/support_attachments.rs),
[Support API](../../crates/proto/proto/erpx/support/v1/service.proto).

### REQ-SUP-004 — Customer-approved support capability

**Requirement.** ERPX shall require a random, hashed, ticket-bound support
capability before a platform agent can access customer business data or perform
represented-user work.

**Rationale.** A support role or ticket alone must not grant customer access.

**Acceptance criteria.** The capability binds customer, agent, device/session,
represented user, mode, permissions, modules, scope, reason, and expiry;
diagnostic read is capped at eight hours and mutation or impersonation at two
hours; raw capability material is not stored.

**Sources.** [Support capabilities](../../adr/ADR-006-support-capabilities.md),
[Support context](../../crates/contexts/support/src/lib.rs).

### REQ-SUP-005 — Independent customer approval

**Requirement.** ERPX shall require an active Holding Owner or Customer
Security Head, separate from the requester, to approve support access with
fresh MFA.

**Rationale.** Customers must retain control over platform access to their data.

**Acceptance criteria.** Self-approval is rejected; the approver is active at
the exact customer scope; requested permissions and duration cannot be
expanded during approval; rejection leaves no usable capability.

**Sources.** [Support capabilities](../../adr/ADR-006-support-capabilities.md),
[Support context](../../crates/contexts/support/src/lib.rs).

### REQ-SUP-006 — Immediate support termination

**Requirement.** ERPX shall terminate effective support authority when the
capability, ticket, customer, role, parent assignment, session, or
customer-hosted agent is no longer valid.

**Rationale.** Support access is the intersection of current constraints, not a
standing grant.

**Acceptance criteria.** Expiry and revocation deny the next operation;
ticket closure terminates dependent sessions; customer-hosted agent disconnect
terminates Root sessions; reissue creates a new capability and never extends
the old one.

**Sources.** [Support capabilities](../../adr/ADR-006-support-capabilities.md),
[support workflow worker](../../apps/worker/src/support_workflow.rs).

### REQ-SUP-007 — Incident and SLA coordination

**Requirement.** ERPX shall provide incident declaration, timeline,
broadcasting, status transition, and SLA notification independently from an
individual support ticket.

**Rationale.** A platform incident can affect many customers and requires a
coordinated operational record.

**Acceptance criteria.** Incidents have severity, state, audience, owner, and
timeline; broadcasts are audience-controlled and idempotent; SLA warning and
breach notifications are durable; incident closure does not silently close
customer tickets.

**Sources.** [Support context](../../crates/contexts/support/src/lib.rs),
[Support API](../../crates/proto/proto/erpx/support/v1/service.proto),
[incident projection](../../apps/worker/src/support_workflow.rs).

### REQ-SUP-008 — Customer-visible support audit

**Requirement.** ERPX shall expose an authorized customer audit of support
approvals, accesses, impersonated actions, downloads, revocations, and
termination outcomes.

**Rationale.** Customers must be able to verify how approved support authority
was used.

**Acceptance criteria.** Every action attributes platform and represented
actors; records identify capability, ticket, scope, purpose, outcome, and
correlation ID; audit excludes secret material and unrelated business data;
revoked access history remains immutable.

**Sources.** [Support capabilities](../../adr/ADR-006-support-capabilities.md),
[audited reads](../../adr/ADR-005-global-read-auditing.md),
[Support API](../../crates/proto/proto/erpx/support/v1/service.proto).
