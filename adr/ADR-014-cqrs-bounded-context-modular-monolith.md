# ADR-014: CQRS bounded-context modular monolith

**Status:** Accepted for implementation

## Decision

ERPX remains a modular monolith with separately deployed API, durable worker,
and customer data-plane-agent processes. Business ownership is enforced by one
transport- and persistence-independent Rust core crate per bounded context,
not by a network service per context.

The shared kernel contains only holding scope, actor/audit attribution, opaque
identities, clocks, and stable error categories. Shared CQRS contracts contain
typed command/query envelopes, command receipts, query/projection metadata,
and handler traits. Cross-context facts are published through versioned
integration contracts; a context may not import another context's aggregate,
repository, application handler, or persistence adapter.

PostgreSQL aggregate snapshots remain authoritative. A command atomically
commits its aggregate state, idempotency receipt, audit evidence, and outbox
event. Workers project outbox events into physically separate, replaceable
read databases. Queries read only those projections. Read-your-writes waits on
a read-side event receipt or checkpoint and returns an explicit not-ready
result when its bounded wait expires; it never falls back to a write table.

The original implementation used `erpx.platform`. ADR-016 completes its
retirement: active packages are now context-specific `erpx.<context>.v2`
services in `IFC-SVC-201..265`, promoted through one coordinated Server,
Worker, Web, and Native cutover. The monolithic source is split by bounded
context, and each context exposes separate typed command and query services.

The cutover creates new control-write, control-read, holding-write, holding-read,
and object-storage resources and copies no ERPX application state. Bootstrap
creates only the mandatory Root/Owner identities, holding/holding registry,
canonical business-unit catalog, and neutral corporate identity. Existing
resources remain offline and non-authoritative for rollback. The n8n state and
encryption configuration under `.runtime/n8n/` are never modified or replaced.

Bootstrap publishes a non-secret, write-once runtime manifest only after the
new namespace, database migrations, restricted seed inventory, holding
activation, and n8n fingerprint recheck succeed. The manifest is not authority
by itself: runtime composition also verifies the matching `complete` database
receipt and active registry row. Development/runtime reconciliation must skip
legacy fixtures and database-profile recreation whenever this fresh-state
handoff is active.

## Bounded contexts

- Control plane: Identity & Access, Platform Administration, Organization,
  Commercial, and Brand.
- Customer-facing: Customer and Support.
- Business sources: Finance, Sales, Procurement, Inventory, Assets,
  Maintenance, Quality, People & Payroll, Point of Sale, and Legal.
- Growth: Marketing and CRM, with the ownership fence defined by ADR-009.
- Orchestration: Operations, which consumes source events and never becomes
  the authority for source transactions.

## Dependency and delivery rules

- Context core crates may depend only on the kernel, CQRS contracts,
  integration contracts, and transport-neutral value libraries.
- SQLx, Tonic/Prost, Dioxus, provider SDKs, and runtime configuration remain in
  adapters or composition roots.
- Context extraction is delivered as vertical slices. Shared workspace
  manifests, protobuf build/export files, migration registries, descriptor
  baselines, and executable composition roots have one integration owner.
- Quality is the pilot slice. Operations and Customer 360 are migrated last
  because their read models consume several source contexts.
- A slice is authoritative only after its invariant, authorization,
  idempotency, atomic-outbox, projection replay/rebuild, transport, and client
  state tests pass.

## Consequences

This decision improves compile-time ownership and independent testing without
adding distributed transactions, network hops, service discovery, or a larger
customer-hosted deployment surface. A bounded context may be extracted into a
microservice later only when production scale, SLA/blast-radius, data-residency,
or independent team ownership supplies measurable evidence for doing so.
