# Product Foundation Requirements

## Product intent

ERPX is a governed enterprise resource planning platform for platform
operators, customer holdings, their companies and branches, and the people who
perform work in twelve canonical business workspaces. The platform combines
business transaction authority, cross-domain orchestration, customer service,
identity, audit, integration, and recovery without splitting ownership across
inconsistent products.

### REQ-FND-001 — Governed enterprise platform

**Requirement.** ERPX shall provide one governed product for platform
administration, customer administration, and the twelve canonical business
workspaces.

**Rationale.** A shared product boundary gives customers consistent identity,
scope, authorization, audit, and operating behavior across business functions.

**Acceptance criteria.** An authorized user can discover only the platform,
customer, and business workspaces available to the active role and scope; the
twelve business workspaces use the canonical names and routes defined by the
product catalog; adding an arbitrary thirteenth core workspace is rejected
until the catalog and its contracts are versioned.

**Sources.** [Business-unit catalog](../crates/domain/src/data_plane.rs),
[bounded-context decision](../adr/ADR-014-cqrs-bounded-context-modular-monolith.md).

### REQ-FND-002 — Explicit organizational hierarchy

**Requirement.** ERPX shall represent Platform, Holding, Company, and Branch as
distinct organizational scopes and shall bind business authority to an exact
scope.

**Rationale.** Customers need delegated administration without allowing a
branch or company action to escape into a parent, sibling, or unrelated
holding.

**Acceptance criteria.** Every protected command and detailed query identifies
its tenant and organizational scope; a branch-scoped actor cannot act on a
sibling branch; parent-scope views include child data only through an
authorized projection or audited gateway.

**Sources.** [Tenant isolation](../adr/ADR-001-tenant-isolation.md),
[branch authority](../adr/ADR-013-holding-branch-database-and-directory-authority.md),
[kernel scope types](../crates/kernel/src/lib.rs).

### REQ-FND-003 — Clear business ownership

**Requirement.** ERPX shall assign each business aggregate and lifecycle to one
authoritative bounded context and shall exchange cross-context facts through
versioned integration contracts.

**Rationale.** Single ownership prevents conflicting totals, ambiguous
approvals, and distributed mutation of the same business record.

**Acceptance criteria.** Each aggregate is mutated by one context only;
Operations cannot mutate source sales orders, purchase orders, stock
movements, invoices, or journals; consumers can process a published fact
without importing the source context's persistence or application layer.

**Sources.** [CQRS bounded contexts](../adr/ADR-014-cqrs-bounded-context-modular-monolith.md),
[operations boundaries](../adr/ADR-008-operations-boundaries.md),
[integration contracts](../crates/integration-contracts/src/lib.rs).

### REQ-FND-004 — Traceable business change

**Requirement.** ERPX shall make every accepted business mutation attributable,
idempotent, version-aware, and traceable to a durable receipt and audit record.

**Rationale.** Enterprise actions must be explainable and safe to retry across
browser, network, worker, and provider failures.

**Acceptance criteria.** Repeating a command with the same identity returns the
same business result without a duplicate mutation; stale aggregate versions
are rejected; successful commands expose a correlation-safe receipt and actor
attribution; failed commands do not leave partial aggregate or outbox state.

**Sources.** [CQRS decision](../adr/ADR-002-cqrs.md),
[CQRS contracts](../crates/cqrs/src/lib.rs),
[contract policy](../CONTRACT_POLICY.md).

### REQ-FND-005 — Managed or customer-hosted data plane

**Requirement.** ERPX shall offer each holding an ERPX-managed data plane and
an opt-in customer-hosted data plane without changing product behavior,
authorization semantics, or the canonical workspace catalog.

**Rationale.** Customers may need local custody of regulated data and
credentials while retaining one governed ERPX product.

**Acceptance criteria.** Both deployment modes expose the same twelve business
workspaces and typed operations; customer-hosted resources are reached only
through the outbound agent; changing deployment mode requires explicit
preflight, approval, recovery validation, and observed-state verification.

**Sources.** [Customer data-plane decision](../adr/ADR-011-customer-data-plane-agent.md),
[data-plane contracts](../crates/proto/proto/erpx/platform_administration/v2/service.proto).

### REQ-FND-006 — Unambiguous cutover authority

**Requirement.** ERPX shall have one mutation owner for each cut-over business
context and shall not operate long-term dual writes with a legacy system.

**Rationale.** Two active write authorities create irreconcilable ordering,
rollback, and audit ambiguity.

**Acceptance criteria.** Migration may shadow and compare legacy changes but
cannot make both systems authoritative; a cutover records the owner switch;
post-cutover legacy writes are rejected; rollback follows an approved,
rehearsed fence rather than merging two independently written histories.

**Sources.** [No dual writes](../adr/ADR-003-no-long-term-dual-writes.md),
[cutover record](../CUTOVER.md),
[migration contracts](../crates/migration/src/g13.rs).

## Target boundary

The product includes business workflows, customer and platform administration,
security, integrations, data custody choices, migration, continuity, and the
shared user experience. Developer tooling is relevant only when it enforces a
target property such as reproducible delivery, compatibility, recovery, or
safe operation.
