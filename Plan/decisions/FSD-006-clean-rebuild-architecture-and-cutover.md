# FSD-006 — Clean Rebuild Architecture and Atomic Cutover

- Status: Accepted
- Owner: Architecture Council
- Effective baseline: 2.0.0
- Supersedes: FSD-001 interface-baseline assumptions where this decision is
  more specific; legacy route, protobuf-service, event, and schema-generation
  compatibility expectations

## Decision

ERPX will rebuild the target runtime in place around 31 bounded contexts using
Domain-Driven Design, Clean Architecture, Hexagonal Architecture, and CQRS as
one enforceable architecture. DDD owns business language, aggregate invariants,
context boundaries, and integration ownership. Clean Architecture requires
dependencies to point toward domain and application policy. Hexagonal
Architecture expresses application dependencies as consumer-owned ports and
keeps HTTP, gRPC, SQLx, workers, Dioxus, and provider SDKs in adapters. CQRS
separates authoritative normalized write models from rebuildable read
projections.

Domain code may depend only on the minimal shared kernel and its own domain
types. It must not depend on SQLx, tonic, prost, Dioxus, HTTP frameworks,
provider SDKs, another context core, or serialized cross-context contracts.
Transport and integration messages are mapped into internal value objects
before invoking domain behavior. Integration-contract packages may depend on
the shared kernel but not on context cores.

Each context exposes one internal Command service and one internal Query
service. Collaboration additionally exposes a stream service, the Item
Directory exposes a reference query service, and Platform Administration
exposes a data-plane stream service. The 65 replacement services use
`IFC-SVC-101..165`; `IFC-SVC-001..065` are retired and never reused.

The target route grammar is singular:

```text
Module:      /<module>
Collection:  /<module>/<record>
Detail:      /<module>/<record>/:id
Copied link: /reference/item/:id

List/query:  GET  /api/v1/<module>/<record>
Create:      POST /api/v1/<module>/<record>
Detail:      GET  /api/v1/<module>/<record>/:id
Command:     POST /api/v1/<module>/<record>/:id/commands/<command>
```

There are no compatibility aliases for `/workspaces`, `/api/bff`,
`/references/:id`, legacy detail routes, legacy service IDs, or legacy event
namespaces. Route scope is never trusted as authorization.

The clean persistence baseline is schema generation v2, beginning at migration
`0001` for newly provisioned control-write, control-read, tenant-write, and
tenant-read databases. Context-owned aggregate data is normalized. JSONB is
limited to naturally document-shaped, versioned payloads. Bootstrap creates new
databases and profiles, rejects unsupported manifests, and never deletes or
mutates the pre-cutover databases or object namespaces.

The release is an atomic hard cutover to one signed artifact and one coherent
route/service/event/schema generation. Rollback switches to the preserved
pre-cutover environment or performs governed forward recovery; it does not
create mixed-version runtime compatibility.

## Consequences

- A framework-free typed Rust catalog becomes the implementation source of
  truth for context IDs, route slugs, record types, service IDs, event
  namespaces, realms, and channel policies. This document remains its
  normative authority until implementation evidence is accepted.
- Every command loads and mutates authoritative state through a write port and
  commits aggregate changes, command receipt, audit, and outbox atomically.
- Every query reads a projection through a read port, returns freshness/source
  position, and never falls back to a write database.
- Cross-context behavior uses versioned integration events, typed handoffs,
  process managers, reconciliation, and compensating outcomes; one context
  never imports or writes another context's domain model.
- Architecture checks must reject inward-dependency violations, sibling-context
  dependencies, query-to-write access, command-invariant reliance on read
  projections, uncataloged route strings, and direct provider access outside
  adapters.
- The source working tree remains historical evidence. Its current routes,
  services, migrations, and implementation state do not override this accepted
  future-state decision.

## Acceptance authority

This decision is verified through E2E-15, E2E-16, E2E-30, E2E-50, and E2E-68,
plus architecture dependency tests, contract-catalog reconciliation, clean
provisioning evidence, projection rebuild evidence, and atomic cutover/rollback
rehearsals.
