# Data, Integrations, and CQRS Requirements

## Data authority and consistency

### REQ-DAT-001 — Authoritative aggregate storage

**Requirement.** ERPX shall keep each bounded context's authoritative
aggregate state in normalized context-owned tables in the write-side
PostgreSQL database selected by the active tenant and branch route.

**Rationale.** A deterministic source of truth is required for mutation,
recovery, and reconciliation.

**Acceptance criteria.** Commands resolve one exact physical route; aggregate
reads for mutation use a write repository port and never a projection; JSONB
is not generic aggregate authority; inactive, suspended, or mismatched routes
fail before mutation; the control plane stores opaque route keys rather than
credentials.

**Sources.** [CQRS decision](../../adr/ADR-002-cqrs.md),
[branch routing](../../adr/ADR-013-holding-branch-database-and-directory-authority.md),
[persistence composition](../../apps/server/src/persistence.rs),
[FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md).

### REQ-DAT-002 — Atomic mutation envelope

**Requirement.** ERPX shall commit aggregate state, expected-version result,
idempotency receipt, audit evidence, and outbox event in one transaction.

**Rationale.** Partial success can lose business events or create untraceable
state.

**Acceptance criteria.** Failure of any element rolls back all elements;
successful state always has its receipt and event; retry with the same command
identity returns the prior result; a different payload under the same identity
is rejected; the application use case reaches persistence only through its
consumer-owned unit-of-work and write-repository ports.

**Sources.** [CQRS decision](../../adr/ADR-002-cqrs.md),
[CQRS contracts](../../crates/cqrs/src/lib.rs),
[SQLx CQRS adapter](../../crates/adapters/cqrs-sqlx/src/lib.rs),
[FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md).

### REQ-DAT-003 — Read-only projections

**Requirement.** ERPX shall answer business queries only through authorized
query use cases and read-repository ports backed by replaceable projections,
and shall not treat projection rows as aggregate authority.

**Rationale.** Read optimization and rebuild must not weaken transactional
ownership.

**Acceptance criteria.** Query handlers use read repositories; projection loss
does not prevent valid writes; query handlers cannot access write repositories;
command handlers cannot use projections to enforce invariants; direct
projection mutation outside a projector is rejected by architecture and
database controls; rebuilding from accepted events reproduces the view.

**Sources.** [CQRS decision](../../adr/ADR-002-cqrs.md),
[bounded-context decision](../../adr/ADR-014-cqrs-bounded-context-modular-monolith.md),
[CQRS SQLx adapter](../../crates/adapters/cqrs-sqlx/src/lib.rs),
[FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md).

### REQ-DAT-004 — Explicit read consistency

**Requirement.** ERPX shall expose projection version, source position, and
freshness and shall return an explicit not-ready result when a bounded
read-your-writes wait expires.

**Rationale.** A user must not mistake delayed data for completed business
processing.

**Acceptance criteria.** Read-your-writes waits on the command event receipt or
checkpoint; timeout never falls back to a write table; unknown freshness is not
reported fresh; clients can distinguish stale, not-ready, unavailable, and
denied.

**Sources.** [Bounded-context decision](../../adr/ADR-014-cqrs-bounded-context-modular-monolith.md),
[CQRS contracts](../../crates/cqrs/src/lib.rs),
[product surfaces](../../apps/console/src/product_completion.rs).

### REQ-DAT-005 — Ordering and duplicate safety

**Requirement.** ERPX shall process outbox and integration events with stable
identity, aggregate ordering, duplicate suppression, bounded retry, and visible
failure state.

**Rationale.** Distributed delivery is at-least-once and can be delayed or
reordered.

**Acceptance criteria.** Duplicate event identity has no second effect;
aggregate sequence gaps do not advance incorrectly; retry count and next
attempt are bounded; exhausted events enter a dead letter with redacted
context; replay resumes from durable state.

**Sources.** [CQRS decision](../../adr/ADR-002-cqrs.md),
[outbox infrastructure](../../crates/infrastructure/src/outbox.rs),
[worker application](../../apps/worker/src/app.rs).

### REQ-DAT-006 — Projection rebuild and replacement

**Requirement.** ERPX shall rebuild projections into a replacement read target,
verify convergence, and activate it without losing authoritative writes.

**Rationale.** Read recovery should be routine and reversible.

**Acceptance criteria.** Rebuild records source boundary and checkpoint;
concurrent new events are caught up; verification compares positions, counts,
and domain checks; a new projection generation activates atomically; the prior
generation remains available for bounded rollback; failed replacement remains
inactive and does not alter the current read route.

**Sources.** [CQRS decision](../../adr/ADR-002-cqrs.md),
[Platform Operations API](../../crates/proto/proto/erpx/platform_administration/v1/service.proto),
[tenant projection scheduler](../../apps/worker/src/tenant_projection_scheduler.rs),
[FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md).

### REQ-DAT-007 — Holding and branch physical isolation

**Requirement.** ERPX shall support `holding_pair` and
`holding_and_branch_pairs` routing while preserving exact tenant, holding,
company, and branch authorization.

**Rationale.** Customers may require branch physical isolation without losing
holding governance.

**Acceptance criteria.** Holding mode routes all business work to the holding
pair; branch-pair mode routes an active exact branch to its own pair; inactive
or ambiguous routes fail closed; holding consolidation consumes branch facts
without using branch credentials in the browser or broker.

**Sources.** [Branch routing](../../adr/ADR-013-holding-branch-database-and-directory-authority.md),
[tenant workspace routing](../../apps/server/src/tenant_workspace.rs).

### REQ-DAT-008 — Domain and cross-context isolation

**Requirement.** ERPX shall keep domain code dependent only on the minimal
kernel and its own domain types, and shall exchange cross-context business
facts only through deliberately mapped, versioned integration contracts.

**Rationale.** Compile-time and contract boundaries preserve business
ownership.

**Acceptance criteria.** Domain and application code contain no SQLx, tonic,
prost, Dioxus, HTTP, provider SDK, or sibling-context core dependency;
transport and cross-context messages map to internal value objects before
domain invocation; integration contracts depend only on the kernel and not
context cores; consumer failure cannot roll back the source transaction;
architecture checks reject violations.

**Sources.** [Bounded-context decision](../../adr/ADR-014-cqrs-bounded-context-modular-monolith.md),
[integration contracts](../../crates/integration-contracts/src/lib.rs),
[architecture checker](../../xtask/src/architecture.rs),
[FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md).

### REQ-DAT-009 — Opaque routing and credential references

**Requirement.** ERPX shall represent database, storage, key, directory, and
provider routing through approved opaque profile keys and shall resolve secret
material only inside the trusted runtime boundary.

**Rationale.** Business and control-plane traffic must not become a credential
distribution channel.

**Acceptance criteria.** APIs reject DSNs and raw credentials; profile key and
scope must match the reserved route; secret lookup occurs in memory through an
approved provider; logs, audits, errors, and read models contain only bounded
failure categories.

**Sources.** [Branch authority](../../adr/ADR-013-holding-branch-database-and-directory-authority.md),
[customer data plane](../../adr/ADR-011-customer-data-plane-agent.md),
[directory secrets](../../apps/worker/src/directory_secrets.rs).

## Interfaces and integrations

### REQ-INT-001 — Versioned typed APIs

**Requirement.** ERPX shall expose exactly one internal Command service and one
internal Query service for each of 31 bounded contexts, plus the three approved
cross-cutting query/stream services, using `IFC-SVC-101..165`.

**Rationale.** Independently deployed Server, Worker, Console, and agent
components need an explicit interoperability boundary.

**Acceptance criteria.** Packages use context-specific versioned namespaces;
Command, Query, and Stream responsibilities remain distinct; field meaning and
numbers are not reused within an active version; IDs `IFC-SVC-001..065` and
the monolithic legacy package are rejected after the coordinated hard cutover.

**Sources.** [Contract policy](../../CONTRACT_POLICY.md),
[active protobuf catalog](../../crates/proto/proto/erpx/catalog/v2/active.proto),
[bounded-context decision](../../adr/ADR-014-cqrs-bounded-context-modular-monolith.md),
[FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md).

### REQ-INT-002 — Provider boundary truthfulness

**Requirement.** ERPX shall report external-provider success only after an
approved adapter returns verifiable provider evidence.

**Rationale.** Configuration, schema presence, or a mock response is not a
business outcome.

**Acceptance criteria.** Unconfigured providers fail with a bounded category;
provider response identity is retained where available; ambiguous timeouts
remain pending reconciliation; tests and local adapters cannot mark external
certification complete.

**Sources.** [Project overview](../../README.md),
[platform completion](../../crates/domain/src/platform_completion.rs),
[marketing provider](../../apps/worker/src/marketing_provider.rs).

### REQ-INT-003 — Retired workflow adapter fence

**Requirement.** ERPX shall reject the retired workflow-adapter kind at domain,
transport, persistence, and migration boundaries. Its former wire value and
catalog identity shall not be reused.

**Rationale.** Removing an adapter must not silently reinterpret durable facts
or make its identifiers available to an unrelated provider.

**Acceptance criteria.** The retired enum value and name are reserved; legacy
guard, projection, receipt, audit, or outbox evidence stops forward migration;
the generic `other` kind cannot spell the retired adapter name.

**Sources.** [Adapter retirement decision](../../adr/ADR-015-retire-n8n-and-unify-local-runtime.md),
[project safety](../../README.md).

### REQ-INT-004 — Stripe webhook integrity

**Requirement.** ERPX shall verify Stripe webhooks over their raw body, persist
them once, process them idempotently and out of order safely, and reconcile
provider state.

**Rationale.** Payment-provider delivery is asynchronous and retried.

**Acceptance criteria.** Invalid signatures are rejected without commercial
mutation; duplicate event IDs have one effect; provider API and webhook version
are pinned; reconciliation corrects local projection without overriding ERPX
catalog or entitlement ownership.

**Sources.** [Stripe authority](../../adr/ADR-007-stripe-authority.md),
[Commercial API](../../crates/proto/proto/erpx/commercial/v1/service.proto),
[commercial infrastructure](../../crates/infrastructure/src/commercial.rs).

### REQ-INT-005 — Native directory feeds

**Requirement.** ERPX shall integrate Generic LDAP through RFC 4533, Microsoft
AD DS through DirSync, and UCS through the Provisioning Service using the
provider's native durable change identity.

**Rationale.** Polling or synthetic cursors cannot reliably prove disable,
delete, and replay behavior.

**Acceptance criteria.** Connector policy binds provider to native feed;
cursor or cookie survives restart; full import and incremental changes are
idempotent; refresh-required or invalid cursor triggers a governed recovery;
unlicensed or unconfigured providers remain unavailable.

**Sources.** [Directory contracts](../../crates/proto/proto/erpx/identity_access/v1/service.proto),
[directory adapters](../../apps/worker/src/directory_adapters.rs),
[directory sync](../../apps/worker/src/directory_sync.rs).

### REQ-INT-006 — Outbound customer data-plane channel

**Requirement.** ERPX shall communicate with a customer-hosted data plane only
through an outbound mTLS agent channel using signed, versioned desired state
and a closed set of typed operations.

**Rationale.** Customers should not expose inbound database or administrative
access to ERPX.

**Acceptance criteria.** Enrollment token is one-time and expiring; agent and
tenant certificate identity match; desired-state version uses optimistic
concurrency; generic SQL, shell, owner credentials, and secret values are not
representable; offline agent rejects new brokered work.

**Sources.** [Customer data-plane decision](../../adr/ADR-011-customer-data-plane-agent.md),
[data-plane API](../../crates/proto/proto/erpx/platform_administration/v1/service.proto),
[data-plane agent](../../apps/data-plane-agent/src/lib.rs).

### REQ-INT-007 — Tenant-bound object storage

**Requirement.** ERPX shall store business and presentation objects in an
approved tenant or control-plane namespace using immutable or versioned keys
and authorized short-lived access.

**Rationale.** Object storage must enforce the same isolation and retention as
the business record.

**Acceptance criteria.** Object key includes the approved namespace and scope;
metadata includes digest and content constraints; signed access is limited by
actor, purpose, operation, and expiry; public access exists only for approved
published presentation assets.

**Sources.** [Tenant isolation](../../adr/ADR-001-tenant-isolation.md),
[corporate identity](../../adr/ADR-012-three-layer-corporate-identity.md),
[server storage](../../apps/server/src/storage.rs).

### REQ-INT-008 — Contract generation evidence

**Requirement.** ERPX shall update checked-in descriptors, typed catalogs, and
contract tests with every internal interface change and shall prove rejection
of retired identifiers at the clean-rebuild boundary.

**Rationale.** Source compilation alone does not prove a coherent atomic
contract generation.

**Acceptance criteria.** Descriptor generation is deterministic; checks compare
the entire artifact against baseline 2.0.0; removed identifiers remain retired
and reserved; all 65 replacement services reconcile to their owners; a legacy
route/service/event/schema probe fails; any mismatch blocks release.

**Sources.** [Contract policy](../../CONTRACT_POLICY.md),
[protobuf build](../../crates/proto/build.rs),
[project verification](../../README.md),
[FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md).
