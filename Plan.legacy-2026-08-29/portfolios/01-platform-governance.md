# Platform Governance Requirements

## Portfolio boundary

This portfolio covers Identity & Access, Platform Administration,
Organization, Commercial, and Brand. Identity and role behavior is specified
in [Stakeholders and access](../02-stakeholders-and-access.md); this document
defines the remaining platform-governance capabilities and their relationship
to that authority.

## Platform Administration

### REQ-PAD-001 — Tenant lifecycle registry

**Requirement.** ERPX shall maintain an authoritative registry of platform
tenants and their lifecycle state, holding binding, deployment mode, and
non-secret resource references.

**Rationale.** Routing and service decisions require one governed customer
registry without storing customer credentials.

**Acceptance criteria.** Tenant registration is idempotent; lifecycle changes
follow defined transitions; suspended tenants fail protected routing; responses
contain opaque resource keys and redacted health rather than DSNs or secrets.

**Sources.** [Platform Administration context](../../crates/contexts/platform-administration/src/lib.rs),
[Platform Administration API](../../crates/proto/proto/erpx/platform_administration/service.proto),
[tenant isolation](../../adr/ADR-001-holding-isolation.md).

### REQ-PAD-002 — Bounded tenant administration context

**Requirement.** ERPX shall issue platform operators a time-bounded tenant
administration context for authorized control-plane operations without granting
general customer business authority.

**Rationale.** Platform maintenance needs an explicit, reviewable boundary.

**Acceptance criteria.** The context identifies actor, tenant, purpose,
permissions, issue and expiry time; ending, expiry, or revocation removes it;
business reads still require an audited gateway or customer-approved support
capability.

**Sources.** [Platform Administration context](../../crates/contexts/platform-administration/src/lib.rs),
[admin-context API](../../crates/proto/proto/erpx/platform_administration/service.proto),
[audited reads](../../adr/ADR-005-global-read-auditing.md).

### REQ-PAD-003 — Connector governance

**Requirement.** ERPX shall register and govern platform and holding connector
bindings by kind, scope, environment, lifecycle, and observed readiness.

**Rationale.** Integrations must not report readiness merely because a schema
or adapter exists.

**Acceptance criteria.** Connector registration validates scope and kind;
activation requires configured authority and observed preflight evidence;
disabled or unconfigured connectors fail closed; external certification is
never synthesized.

**Sources.** [Platform Administration context](../../crates/contexts/platform-administration/src/lib.rs),
[platform completion](../../crates/domain/src/platform_completion.rs),
[worker completion runtime](../../apps/worker/src/platform_completion_runtime.rs).

### REQ-PAD-004 — Projection administration

**Requirement.** ERPX shall let authorized operators inspect projection
freshness, start a rebuild, verify replacement convergence, and atomically
activate a replacement read model.

**Rationale.** Read models must be recoverable without placing authoritative
writes at risk.

**Acceptance criteria.** Status shows source and checkpoint position,
freshness, and failure category; rebuild writes to a replacement target;
activation is blocked until verification succeeds; queries never fall back to
write tables during recovery.

**Sources.** [Platform Operations API](../../crates/proto/proto/erpx/platform_administration/service.proto),
[CQRS decision](../../adr/ADR-002-cqrs.md),
[tenant projection scheduler](../../apps/worker/src/holding_projection_scheduler.rs).

### REQ-PAD-005 — Platform audit and dead-letter operations

**Requirement.** ERPX shall provide authorized platform views for immutable
audit entries, worker readiness, and bounded dead-letter replay.

**Rationale.** Operators need repair capabilities that preserve attribution and
do not bypass the original business boundary.

**Acceptance criteria.** Audit and dead-letter details are redacted and
filterable; replay requires exact event identity, authorization, reason, and
idempotency; successful replay cannot duplicate an already applied projection.

**Sources.** [Platform Operations API](../../crates/proto/proto/erpx/platform_administration/service.proto),
[worker application](../../apps/worker/src/app.rs),
[CQRS decision](../../adr/ADR-002-cqrs.md).

### REQ-PAD-006 — Migration control

**Requirement.** ERPX shall expose governed migration status and phase
transitions without allowing an operator to skip required backup,
reconciliation, approval, or rollback fences.

**Rationale.** Migration progress is a business-control workflow, not an
unrestricted infrastructure command.

**Acceptance criteria.** A run records source snapshot, phase, counts, digests,
watermarks, approvals, and recovery point; invalid phase advancement is
rejected; owner switch is unavailable until the final reconciliation and
approval pass.

**Sources.** [Migration API](../../crates/proto/proto/erpx/platform_administration/service.proto),
[migration runtime](../../apps/worker/src/g13_migration_runtime.rs),
[no dual writes](../../adr/ADR-003-no-long-term-dual-writes.md).

## Organization

### REQ-ORG-001 — Canonical organization hierarchy

**Requirement.** ERPX shall maintain versioned Holding, Company, Branch, and
other approved organization nodes with explicit placement and status.

**Rationale.** Business scope and reporting depend on a trustworthy hierarchy.

**Acceptance criteria.** Node codes are unique within their defined boundary;
reparenting cannot create cycles or cross holdings; rename and code changes
retain identity and history; archived nodes cannot receive new operational
work.

**Sources.** [Organization context](../../crates/contexts/organization/src/lib.rs),
[Organization API](../../crates/proto/proto/erpx/organization/service.proto).

### REQ-ORG-002 — Provisioned branch creation

**Requirement.** ERPX shall create an active branch only through the governed
branch-provisioning workflow, not through a general organization-node save.

**Rationale.** An active branch may require physical databases, projection,
backup, directory, and route setup that a simple hierarchy mutation cannot
guarantee.

**Acceptance criteria.** Provisioning reserves scope and resource keys before
creation; partial resources remain inactive after failure; only verified
write/read databases and selected business units can be activated; retry
requires fresh MFA and explicit authorization.

**Sources.** [Branch authority](../../adr/ADR-013-holding-branch-database-and-directory-authority.md),
[branch provisioning runtime](../../apps/worker/src/branch_provisioning_runtime.rs),
[Organization context](../../crates/contexts/organization/src/lib.rs).

### REQ-ORG-003 — Organization lifecycle controls

**Requirement.** ERPX shall support authorized activation, suspension,
archival, rename, code change, and reparent operations while preserving
referential and authorization integrity.

**Rationale.** Structural changes must not orphan business records or silently
expand access.

**Acceptance criteria.** Every transition is version-checked and audited;
suspension blocks new work and route resolution as configured while retaining
recovery data; reparenting re-evaluates inherited authority; historical records
keep their original scope attribution.

**Sources.** [Organization context](../../crates/contexts/organization/src/lib.rs),
[organization domain](../../crates/contexts/organization/src/domain/mod.rs).

### REQ-ORG-004 — Fixed business-unit allocation

**Requirement.** ERPX shall configure a branch from the canonical business-unit
catalog and shall not let a branch create an independent core connector,
database type, or unregistered business unit.

**Rationale.** Branch specialization must remain compatible with holding-wide
identity and product governance.

**Acceptance criteria.** A branch can enable an approved subset of the twelve
units; every enabled unit uses the active branch or holding route; disabled
units cannot accept commands; allocation changes are versioned and authorized.

**Sources.** [Business-unit catalog](../../crates/domain/src/data_plane.rs),
[customer data plane](../../adr/ADR-011-customer-data-plane-agent.md),
[branch authority](../../adr/ADR-013-holding-branch-database-and-directory-authority.md).

## Commercial and entitlement

### REQ-COM-001 — Versioned product catalog

**Requirement.** ERPX shall own immutable published catalog revisions and the
mapping between ERPX plans, capacities, and provider products.

**Rationale.** Commercial meaning must remain stable even when provider catalog
objects change.

**Acceptance criteria.** Draft plans can change before publication; published
revisions cannot be edited in place; subscriptions retain the applicable
revision or an explicit grandfathering decision; provider mappings are
versioned and reconcilable.

**Sources.** [Commercial context](../../crates/contexts/commercial/src/lib.rs),
[Commercial API](../../crates/proto/proto/erpx/commercial/service.proto),
[Stripe authority](../../adr/ADR-007-stripe-authority.md).

### REQ-COM-002 — Provider-authoritative billing state

**Requirement.** ERPX shall treat verified Stripe payment and subscription
lifecycle facts as authoritative for provider billing state while retaining
ERPX authority over catalog and entitlement policy.

**Rationale.** Local state must not falsely override a payment-provider outcome.

**Acceptance criteria.** Webhooks are signature-verified from the raw body,
inboxed, idempotent, and safe out of order; uncertain events remain pending
until reconciliation; local commands cannot mark an unpaid provider
subscription paid.

**Sources.** [Stripe authority](../../adr/ADR-007-stripe-authority.md),
[Commercial context](../../crates/contexts/commercial/src/lib.rs),
[SaaS billing API](../../crates/proto/proto/erpx/commercial/service.proto).

### REQ-COM-003 — Entitlement allocation

**Requirement.** ERPX shall grant product capabilities through versioned,
scope-bound entitlements with explicit capacity and allocation rules.

**Rationale.** Billing purchase and operational authorization are related but
not interchangeable.

**Acceptance criteria.** Entitlement use cannot exceed active capacity;
allocations are bound to the correct holding or child scope; expired,
suspended, or revoked entitlements deny new mutations; entitlement changes are
audited and projected.

**Sources.** [Commercial context](../../crates/contexts/commercial/src/lib.rs),
[entitlement API](../../crates/proto/proto/erpx/identity_access/service.proto).

### REQ-COM-004 — Payment-failure protection

**Requirement.** ERPX shall apply a governed grace and read-only policy when
provider payment state removes mutation entitlement.

**Rationale.** Customers need predictable continuity without receiving unpaid
write authority.

**Acceptance criteria.** The default grace window is seven days unless an
approved policy says otherwise; safe read capabilities remain explicit;
mutations outside the grace policy are denied; recovery restores only
permissions and entitlements that are currently valid.

**Sources.** [Stripe authority](../../adr/ADR-007-stripe-authority.md),
[Commercial context](../../crates/contexts/commercial/src/lib.rs).

## Corporate identity and brand

### REQ-BRD-001 — Three-layer corporate identity

**Requirement.** ERPX shall manage versioned corporate identity at Platform,
Holding, and Business Unit scopes.

**Rationale.** Customers need recognizable presentation without replacing
ERPX security and product semantics.

**Acceptance criteria.** Resolution applies safety tokens, platform profile,
holding overrides, business-unit overrides, and user preferences in order;
the resolved result identifies its contributing versions; an absent override
inherits the parent.

**Sources.** [Corporate identity decision](../../adr/ADR-012-three-layer-corporate-identity.md),
[Brand context](../../crates/contexts/brand/src/lib.rs),
[Brand API](../../crates/proto/proto/erpx/brand/service.proto).

### REQ-BRD-002 — Parent category locks

**Requirement.** ERPX shall allow Platform and Holding profiles to lock
approved identity categories against child overrides.

**Rationale.** Parent governance must remain enforceable across brands and
business units.

**Acceptance criteria.** A child draft cannot publish a locked category;
unlocking is separately authorized and versioned; safety/status colors, focus,
destructive styling, exceptional-access banners, and Root identity are always
locked by product policy.

**Sources.** [Corporate identity decision](../../adr/ADR-012-three-layer-corporate-identity.md),
[Brand context](../../crates/contexts/brand/src/lib.rs).

## Lifecycle-completion requirements

| ID | Title | Normative requirement | Rationale | Acceptance paths | Source | Scenarios |
|---|---|---|---|---|---|---|
| REQ-PAD-090 | Administration lifecycle coverage | ERPX shall govern configuration, request, approval, activation, suspension, correction, closure, and archive for every Platform Administration aggregate. | Control-plane changes need complete states. | Success: valid transition emits receipt; Denial: invalid/self-approved transition is rejected; Failure: partial operation remains resumable and inactive. | [FSD-003](../decisions/FSD-003-enterprise-suite-expansion.md) | E2E-01, E2E-02, E2E-30 |
| REQ-PAD-091 | Administration reconciliation | ERPX shall reconcile provider, worker, route, projection, migration, and audit outcomes before administrative completion. | Requested control work may fail asynchronously. | Success: accepted outcomes converge; Denial: operator cannot mark provider success manually; Failure: mismatch enters governed work. | [FSD-003](../decisions/FSD-003-enterprise-suite-expansion.md) | E2E-02, E2E-15, E2E-30 |
| REQ-ORG-090 | Organization lifecycle coverage | ERPX shall govern create, approve, activate, change, move, suspend, close, and archive of organization units with effective dates. | Hierarchy changes affect every domain. | Success: permitted transition updates versioned hierarchy; Denial: child/sibling escape is rejected; Failure: incomplete provisioning prevents activation. | [FSD-003](../decisions/FSD-003-enterprise-suite-expansion.md) | E2E-02, E2E-48 |
| REQ-ORG-091 | Organization reconciliation | ERPX shall reconcile organization changes with routes, directories, entitlements, references, work, and retention. | A hierarchy change has downstream effects. | Success: all consumers reach source version; Denial: stale consumer cannot grant access; Failure: gap remains visible and retryable. | [FSD-003](../decisions/FSD-003-enterprise-suite-expansion.md) | E2E-02, E2E-68 |
| REQ-COM-090 | Commercial lifecycle coverage | ERPX shall govern catalog, offer, subscription, entitlement, billing-state, suspension, correction, cancellation, and archive transitions. | Commercial authority needs full lifecycle. | Success: valid transition pins versions; Denial: provider-unconfirmed state cannot be asserted; Failure: partial provider outcome reconciles. | [FSD-003](../decisions/FSD-003-enterprise-suite-expansion.md) | E2E-01, E2E-18 |
| REQ-COM-091 | Commercial reconciliation | ERPX shall reconcile provider billing, ERPX entitlement, tenant route, invoice, and payment outcomes. | Billing and access must not drift. | Success: source authorities agree; Denial: manual success override is rejected; Failure: mismatch protects access/state per policy. | [FSD-003](../decisions/FSD-003-enterprise-suite-expansion.md) | E2E-18 |
| REQ-BRD-090 | Brand lifecycle coverage | ERPX shall govern draft, review, approve, publish, schedule, withdraw, rollback, retire, and archive for brand bundles and assets. | Public identity changes need complete control. | Success: exact version publishes; Denial: unapproved asset is unavailable; Failure: partial publication rolls back or uses last verified bundle. | [FSD-003](../decisions/FSD-003-enterprise-suite-expansion.md) | E2E-01, E2E-17 |
| REQ-BRD-091 | Brand reconciliation | ERPX shall reconcile holding/company/branch inheritance, asset objects, public caches, and route publication. | Three-layer identity must converge. | Success: resolved bundle matches hierarchy; Denial: child cannot override locked parent token; Failure: stale cache is invalidated safely. | [FSD-003](../decisions/FSD-003-enterprise-suite-expansion.md) | E2E-17 |

### REQ-BRD-003 — Governed publication and rollback

**Requirement.** ERPX shall require exact-scope permission, fresh MFA, reason,
optimistic concurrency, immutable audit, and outbox evidence to publish or
rollback a brand profile.

**Rationale.** Public presentation changes can affect trust and accessibility.

**Acceptance criteria.** A stale draft cannot publish; ERPX Root may propose
but cannot silently publish customer branding; rollback creates a new
traceable revision rather than deleting history; all target clients resolve
the same published version.

**Sources.** [Corporate identity decision](../../adr/ADR-012-three-layer-corporate-identity.md),
[Brand context](../../crates/contexts/brand/src/lib.rs).

### REQ-BRD-004 — Safe public brand resolution

**Requirement.** ERPX shall resolve only published, non-secret presentation
metadata for a verified hostname and shall return the neutral ERPX profile for
an unknown hostname.

**Rationale.** Pre-login branding must not reveal tenant existence or enable a
counterfeit login surface.

**Acceptance criteria.** Public brand responses contain no credentials,
infrastructure references, business data, scripts, or arbitrary CSS; the login
retains ERPX security attribution; unknown and unregistered hostnames receive
the same neutral outcome.

**Sources.** [Corporate identity decision](../../adr/ADR-012-three-layer-corporate-identity.md),
[console BFF](../../crates/console-bff/src/lib.rs).

### REQ-BRD-005 — Governed brand assets

**Requirement.** ERPX shall accept brand assets only as immutable,
digest-addressed control-plane objects and fonts only from bundled licensed
font packs.

**Rationale.** Mutable or remote assets can break availability, licensing,
privacy, and pre-login safety.

**Acceptance criteria.** Asset references use approved object keys and digest;
expired external URLs and remote font imports are rejected; clients map typed
tokens to a fixed allow-list; the last valid public bundle can be used for
offline presentation.

**Sources.** [Corporate identity decision](../../adr/ADR-012-three-layer-corporate-identity.md),
[Brand context](../../crates/contexts/brand/src/lib.rs).
