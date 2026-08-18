# Security, Privacy, and Audit Requirements

## Authentication and authorization

### REQ-SEC-001 — Governed authentication methods

**Requirement.** ERPX shall support password, TOTP, recovery code, passkey, and
approved directory authentication through backend-verified flows.

**Rationale.** Users need resilient authentication without allowing clients to
self-assert identity or MFA.

**Acceptance criteria.** Authentication secrets are submitted only to approved
same-origin or native secure boundaries; TOTP and recovery codes are one-time
where applicable; passkey challenge and origin are verified; directory
authentication follows connector trust and scope policy.

**Sources.** [Identity crate](../../crates/identity/src/lib.rs),
[console authentication](../../apps/console/src/composition.rs),
[identity API](../../crates/proto/proto/erpx/identity_access/v2/service.proto).

### REQ-SEC-002 — Protected sessions and devices

**Requirement.** ERPX shall bind protected sessions to active principal,
device, role context, issue and expiry times, and revocation state.

**Rationale.** Authentication at login must not remain valid after device,
role, or principal authority ends.

**Acceptance criteria.** Session refresh re-evaluates active identity and
context; device revocation terminates dependent sessions; Web avoids plaintext
token persistence; native clients use secure storage; logout and revocation
are idempotent.

**Sources.** [Web session](../../crates/web-session/src/lib.rs),
[session SQLx adapter](../../crates/adapters/web-session-sqlx/src/lib.rs),
[Dioxus target strategy](../../adr/ADR-010-dioxus-target-strategy.md).

### REQ-SEC-003 — Authoritative reauthorization

**Requirement.** ERPX shall reauthorize every protected command and detailed
query at its authoritative boundary using current identity, session, role,
scope, entitlement, risk, and exceptional capability state.

**Rationale.** Navigation visibility and prior access cannot authorize a later
operation.

**Acceptance criteria.** Server denial wins over client metadata; expired or
revoked authority denies the next request; wrong-tenant and wrong-branch access
fail without confirming protected record existence; authorization outcome is
auditable.

**Sources.** [Universal RBAC](../../adr/ADR-004-expiring-rbac.md),
[navigation model](../../crates/ui/src/navigation.rs),
[server boundary](../../apps/server/src/boundary.rs).

### REQ-SEC-004 — Complete effective-access evaluation

**Requirement.** ERPX shall calculate effective access from active identity and
session, assignment and parent validity, scope containment, permission,
entitlement, billing state, approval limit, separation of duties, risk, and any
support capability.

**Rationale.** Permission alone is not sufficient enterprise authorization.

**Acceptance criteria.** Failure of any required factor denies the operation;
evaluation is deterministic at a stated time; no permanent assignment is
representable; the decision exposes a bounded reason without leaking policy or
secret detail.

**Sources.** [Universal RBAC](../../adr/ADR-004-expiring-rbac.md),
[access domain](../../crates/domain/src/access.rs),
[identity-access context](../../crates/contexts/identity-access/src/lib.rs).

### REQ-SEC-005 — Fresh MFA and maker-checker

**Requirement.** ERPX shall require fresh MFA and independent approver evidence
for configured high-risk identity, access, finance, support, data, recovery,
migration, and policy actions.

**Rationale.** High-impact actions require protection against stale sessions
and single-actor compromise.

**Acceptance criteria.** MFA is verified by the backend at action time;
requester and required approver differ; authority and scope remain current;
replayed, stale, or previously consumed evidence is rejected.

**Sources.** [Universal RBAC](../../adr/ADR-004-expiring-rbac.md),
[support capabilities](../../adr/ADR-006-support-capabilities.md),
[approval requirements](../04-approval-governance.md).

### REQ-SEC-006 — Tenant and branch isolation

**Requirement.** ERPX shall enforce physical holding or branch isolation and
logical company and branch scope at database, object, projection, worker, API,
and UI boundaries.

**Rationale.** Isolation must survive mistakes in any single presentation or
service layer.

**Acceptance criteria.** Wrong-tenant credentials and routes fail; RLS and
repositories require explicit scope; object keys and signed access are scoped;
workers cannot claim unrelated tenant work; cross-tenant reads use an audited
gateway rather than credential fan-out.

**Sources.** [Tenant isolation](../../adr/ADR-001-tenant-isolation.md),
[branch authority](../../adr/ADR-013-holding-branch-database-and-directory-authority.md),
[tenant workspace](../../apps/server/src/tenant_workspace.rs).

### REQ-SEC-007 — Customer-hosted Root admission

**Requirement.** ERPX shall require a new customer approval and fresh MFA for
every interactive Root session into a customer-hosted data plane, with the
customer agent as final admission authority.

**Rationale.** Platform emergency or infrastructure labels must not override
customer custody.

**Acceptance criteria.** Capability binds Root principal, device, tenant,
agent, scope, units, operations, and TTL; signed capability is one-time and
delivered over the outbound channel; rejection, expiry, revocation, missing
approval, or agent disconnection denies access.

**Sources.** [Support capabilities](../../adr/ADR-006-support-capabilities.md),
[customer data plane](../../adr/ADR-011-customer-data-plane-agent.md).

### REQ-SEC-008 — Secret exclusion

**Requirement.** ERPX shall keep passwords, database URLs, storage keys,
directory bind credentials, provider secrets, tokens, private keys, and payment
instruments out of domain state, browser state, APIs, audits, diagnostics, and
logs.

**Rationale.** Observability and business records must not become a secret
store.

**Acceptance criteria.** APIs accept profile keys rather than secret values;
secrets resolve only within trusted runtime adapters; error and debug output are
redacted; automated scans reject credential-bearing responses and logs.

**Sources.** [Tenant isolation](../../adr/ADR-001-tenant-isolation.md),
[customer data plane](../../adr/ADR-011-customer-data-plane-agent.md),
[directory secrets](../../apps/worker/src/directory_secrets.rs).

### REQ-SEC-009 — Directory trust and fail-closed fallback

**Requirement.** ERPX shall use TLS-validated directory authority and shall
fail closed on invalid credentials, disabled or deleted accounts, policy
denial, trust error, or directory fingerprint drift.

**Rationale.** Availability fallback must never bypass identity authority.

**Acceptance criteria.** Server certificate and hostname are validated;
holding fallback is considered only for availability failure and matching
fingerprint; effective roles are recalculated from branch mappings; every
accepted and rejected fallback is customer-visible in audit.

**Sources.** [Branch directory authority](../../adr/ADR-013-holding-branch-database-and-directory-authority.md),
[directory live adapter](../../apps/worker/src/directory_live.rs),
[directory auth](../../crates/identity/src/directory/auth.rs).

### REQ-SEC-010 — Five-minute directory revocation

**Requirement.** ERPX shall remove effective access for a provider-side
disable, delete, or mapped-role revocation within five minutes of authoritative
feed observation and shall fail closed when feed freshness exceeds policy.

**Rationale.** Directory-managed access must terminate within a measurable
security window.

**Acceptance criteria.** Feed observation and access cutoff times are durable;
append-only role facts preserve cause; stale-feed policy denies affected
directory access; restart and replay do not extend the deadline.

**Sources.** [Product security contracts](../../apps/console/src/product_completion.rs),
[directory sync](../../apps/worker/src/directory_sync.rs),
[directory worker tests](../../apps/worker/tests/worker_directory.rs).

## Audit

### REQ-AUD-001 — Immutable mutation audit

**Requirement.** ERPX shall record immutable audit evidence for every accepted
or rejected protected mutation.

**Rationale.** Stakeholders need to explain who attempted what, under which
authority, and with what outcome.

**Acceptance criteria.** Audit records actor and represented actor, scope,
action, aggregate reference, prior and resulting version where applicable,
purpose or reason, time, correlation, and outcome; secrets and prohibited data
are excluded; entries cannot be edited by business commands.

**Sources.** [CQRS decision](../../adr/ADR-002-cqrs.md),
[platform audit migration](../../crates/migration/migrations/0003_platform_audits.sql),
[CQRS contracts](../../crates/cqrs/src/lib.rs).

### REQ-AUD-002 — Audited global reads

**Requirement.** ERPX shall audit every platform detailed customer, module, or
scope read through the global read gateway.

**Rationale.** Customer data access must be visible even when it does not
change data.

**Acceptance criteria.** Detail audit contains actor, customer, module, scope,
purpose, time, correlation, and outcome; list discovery has separate bounded
semantics; bulk export and sensitive download require explicit approved
authority.

**Sources.** [Audited global reads](../../adr/ADR-005-global-read-auditing.md),
[Customer access audit API](../../crates/proto/proto/erpx/customer_authority/v2/service.proto).

### REQ-AUD-003 — Customer-visible access history

**Requirement.** ERPX shall project relevant platform, support, directory, and
global-read audit events into an authorized customer security view.

**Rationale.** Customers should not rely on platform staff to disclose access
history manually.

**Acceptance criteria.** View is tenant-bound and rebuildable; entries preserve
source audit identity and ordering; restricted payload is redacted without
removing outcome; projection lag is visible.

**Sources.** [Audited global reads](../../adr/ADR-005-global-read-auditing.md),
[customer access audit worker test](../../apps/worker/tests/customer_access_audit.rs).

### REQ-AUD-004 — Dual-actor attribution

**Requirement.** ERPX shall attribute both the platform actor and represented
customer actor for impersonated or represented-user operations.

**Rationale.** Neither platform involvement nor customer authority may be
hidden.

**Acceptance criteria.** Both actor IDs appear in command and read audit;
permissions reflect the intersection of the capability and represented user;
customer-visible history labels the exceptional session; termination does not
alter prior attribution.

**Sources.** [Support capabilities](../../adr/ADR-006-support-capabilities.md),
[Support context](../../crates/contexts/support/src/lib.rs).

## Privacy

### REQ-PRI-001 — Purpose-bound data minimization

**Requirement.** ERPX shall return and retain only the customer, employee,
legal, marketing, support, and provider fields required for the authorized
business purpose.

**Rationale.** Broad projections and diagnostics increase privacy and breach
impact.

**Acceptance criteria.** List and detail field sets are explicit; secrets and
unrelated business data are excluded; support diagnostics are bounded;
denied fields are not inferable from hidden payload or counts.

**Sources.** [Audited global reads](../../adr/ADR-005-global-read-auditing.md),
[product display contracts](../../apps/console/src/product_completion.rs).

### REQ-PRI-002 — Consent and suppression

**Requirement.** ERPX shall enforce purpose- and channel-specific marketing
consent and durable suppression at audience selection and immediately before
delivery.

**Rationale.** Later withdrawal must override earlier campaign eligibility.

**Acceptance criteria.** Consent and suppression are versioned; missing consent
fails closed; suppression cancels queued delivery; retained suppression does
not expose unnecessary personal profile data.

**Sources.** [Marketing boundaries](../../adr/ADR-009-marketing-crm-boundaries.md),
[marketing delivery worker](../../apps/worker/src/marketing_delivery.rs).

### REQ-PRI-003 — Governed erasure

**Requirement.** ERPX shall execute authorized erasure or anonymization by
scope and purpose while respecting active legal holds, financial records,
security audit, and suppression obligations.

**Rationale.** Privacy removal cannot erase records that must legally or
security-wise remain, nor permit future prohibited contact.

**Acceptance criteria.** Erasure plan identifies affected stores and lawful
retentions; active hold blocks covered removal; execution is idempotent;
minimum retained facts are non-reversible and purpose-limited; completion is
audited.

**Sources.** [Marketing privacy worker](../../apps/worker/src/marketing_content_privacy.rs),
[Legal context](../../crates/contexts/legal/src/lib.rs),
[marketing privacy migration](../../crates/migration/migrations_tenant_read/0039_marketing_profile_erasure.sql).

### REQ-PRI-004 — Retention enforcement

**Requirement.** ERPX shall apply versioned retention policy to business,
audit, object, backup, support, and legal records and shall distinguish archive,
expiry, erasure, and legal hold.

**Rationale.** Different record classes require different preservation and
deletion outcomes.

**Acceptance criteria.** Retention owner and horizon are explicit; background
processing is bounded and restartable; hold overrides ordinary deletion;
backup expiry does not delete active authority data; material deletion records
recoverability and audit evidence.

**Sources.** [Legal context](../../crates/contexts/legal/src/lib.rs),
[backup retention worker](../../apps/worker/src/backup_retention.rs),
[support domain](../../crates/domain/src/support.rs).

## Expanded security and privacy governance

| ID | Title | Normative requirement | Rationale | Acceptance paths | Source | Scenarios |
|---|---|---|---|---|---|---|
| REQ-SEC-011 | Realm isolation | ERPX shall isolate workforce/admin, commerce/customer/guest, candidate, signatory, agent, and service authentication realms. | Shared identifiers must not transfer authority. | Success: explicit verified link preserves separate credentials/roles; Denial: email/phone match grants nothing; Failure: link conflict fails closed. | [FSD-002](../decisions/FSD-002-shared-platform-capabilities.md) | E2E-26, E2E-36, E2E-37 |
| REQ-SEC-012 | Key lifecycle | ERPX shall govern generation, storage reference, use, rotation, revocation, recovery, and destruction for cryptographic keys. | Encryption depends on controlled keys. | Success: active key use is attributable; Denial: raw key is excluded from business state/logs; Failure: rotation gap blocks unsafe operation. | [FSD-002](../decisions/FSD-002-shared-platform-capabilities.md) | E2E-30 |
| REQ-SEC-013 | Cross-capability revocation | ERPX shall propagate identity and record revocation to sessions, work, comments, notifications, references, search, exports, and agent execution. | New shared capabilities cannot retain stale access. | Success: next operation is denied; Denial: cached token/reference grants nothing; Failure: lag quarantines affected capability. | [FSD-005](../decisions/FSD-005-collaboration-references-and-agents.md) | E2E-26, E2E-31, E2E-68 |
| REQ-PRI-005 | Four-tier classification | ERPX shall classify business data as Public, Internal, Confidential, or Restricted and shall prohibit secrets from business state. | Consistent tiers drive controls. | Success: every catalog field resolves a tier; Denial: unclassified protected field cannot publish; Failure: ambiguity selects the safer restriction. | [FSD-002](../decisions/FSD-002-shared-platform-capabilities.md) | E2E-42, E2E-61 |
| REQ-PRI-006 | Data ownership | ERPX shall assign owner, steward, purpose, subjects, locations, consumers, and lifecycle to each governed dataset. | Accountability precedes processing. | Success: catalog metadata is complete; Denial: ownerless dataset cannot activate; Failure: owner departure triggers review. | [FSD-002](../decisions/FSD-002-shared-platform-capabilities.md) | E2E-42 |
| REQ-PRI-007 | Residency and sharing | ERPX shall evaluate residency, transfer, recipient, provider, DPA, and purpose before third-party data sharing. | Authorization alone does not permit egress. | Success: approved transfer records decision; Denial: unmatched destination is blocked; Failure: policy outage fails closed. | [FSD-004](../decisions/FSD-004-thailand-and-sector-overlays.md) | E2E-42, E2E-61 |
| REQ-PRI-008 | Breach evidence | ERPX shall preserve scoped detection, containment, assessment, notification decision, remediation, and closure evidence for privacy incidents. | Incident handling needs regulatory evidence. | Success: authorized case reconstructs timeline; Denial: ordinary users cannot inspect affected identities; Failure: missing facts remain open/escalated. | [FSD-004](../decisions/FSD-004-thailand-and-sector-overlays.md) | E2E-42 |

## Secure product assurance

| ID | Title | Normative requirement | Rationale | Acceptance paths | Source | Scenarios |
|---|---|---|---|---|---|---|
| REQ-SQA-001 | Threat model | ERPX shall maintain versioned threat models for every context, trust boundary, provider, and high-risk journey. | Security design needs explicit threats. | Success: model maps controls/tests; Denial: material feature without review cannot certify; Failure: changed boundary triggers reassessment. | [FSD-002](../decisions/FSD-002-shared-platform-capabilities.md) | E2E-30 |
| REQ-SQA-002 | Security requirements review | ERPX shall require Security and Privacy review for changes affecting identity, isolation, Restricted data, agents, providers, or exports. | High-risk changes need specialist sign-off. | Success: approved review pins scope; Denial: missing sign-off blocks baseline acceptance; Failure: changed design invalidates review. | [FSD-001](../decisions/FSD-001-baseline-and-governance.md) | E2E-30 |
| REQ-SQA-003 | Dependency inventory | ERPX shall produce an SBOM covering shipped components, versions, licenses, origin, and integrity. | Supply-chain exposure must be known. | Success: artifact maps to build digest; Denial: unknown component blocks verified release; Failure: incomplete inventory is reported as failure. | [FSD-002](../decisions/FSD-002-shared-platform-capabilities.md) | E2E-30 |
| REQ-SQA-004 | Build provenance | ERPX shall bind source revision, toolchain, dependencies, inputs, tests, signer, and artifact digest in build provenance. | Customers need artifact authenticity. | Success: signature verifies manifest; Denial: unsigned/mismatched artifact cannot promote; Failure: missing provenance blocks release. | [FSD-002](../decisions/FSD-002-shared-platform-capabilities.md) | E2E-30 |
| REQ-SQA-005 | Security testing | ERPX shall test authentication, authorization, isolation, injection, serialization, file handling, provider callbacks, and abuse paths proportionate to risk. | Positive tests alone are insufficient. | Success: required suite produces evidence; Denial: waived test needs approved exception; Failure: failed control blocks sign-off. | [FSD-002](../decisions/FSD-002-shared-platform-capabilities.md) | E2E-30, E2E-67 |
| REQ-SQA-006 | Secret scanning | ERPX shall prevent credentials, tokens, private keys, and regulated secrets from source, artifacts, logs, fixtures, and evidence. | Secrets must not enter retained materials. | Success: clean scan is evidenced; Denial: detected secret blocks promotion and rotates affected credential; Failure: scanner outage blocks verified release. | [FSD-002](../decisions/FSD-002-shared-platform-capabilities.md) | E2E-30 |
| REQ-SQA-007 | Vulnerability evidence | ERPX shall record affected component, exploitability, exposure, decision, remediation, verification, and exception evidence without inventing global deadlines. | Risk handling must be accountable and policy-driven. | Success: disposition is approved and verified; Denial: unresolved critical policy breach blocks release; Failure: unknown exposure remains open. | [FSD-002](../decisions/FSD-002-shared-platform-capabilities.md) | E2E-30 |
| REQ-SQA-008 | Penetration evidence | ERPX shall retain scoped independent security-assessment findings, reproductions, remediation, retest, and accepted residual risk. | Independent assurance complements tests. | Success: retest closes addressed finding; Denial: unapproved residual risk cannot certify; Failure: evidence gap keeps finding open. | [FSD-002](../decisions/FSD-002-shared-platform-capabilities.md) | E2E-30 |
| REQ-SQA-009 | Security incident | ERPX shall coordinate detection, containment, evidence preservation, recovery, communication, and post-incident action under role authority. | Incidents need an auditable lifecycle. | Success: exercised runbook records decisions; Denial: unauthorized actor cannot suppress evidence; Failure: missing telemetry escalates degraded response. | [FSD-002](../decisions/FSD-002-shared-platform-capabilities.md) | E2E-19, E2E-67 |
| REQ-SQA-010 | Certification boundary | ERPX shall distinguish product assurance evidence from formal certification and legal compliance. | Strong tests do not equal certification. | Success: claim names external authority/scope; Denial: unsupported badge is unavailable; Failure: material change triggers recertification. | [FSD-004](../decisions/FSD-004-thailand-and-sector-overlays.md) | E2E-42, E2E-44 |
