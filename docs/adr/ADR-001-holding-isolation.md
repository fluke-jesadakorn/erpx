# ADR-001: Holding isolation

> **Partially superseded by ADR-023 (2026-09-06).** Physical Holding isolation remains authoritative; the clause making `HoldingId` canonical in public routing is superseded by immutable public slugs and server-side binding resolution.


> **Implementation boundary:** managed local Node/PostgreSQL is the current deployed mode ([ADR-024](ADR-024-local-postgresql-runtime.md), [ADR-032](ADR-032-shared-local-postgresql-cluster.md)). CustomerDataPlane/agent clauses retain the architectural target in [ADR-011](ADR-011-customer-data-plane-agent.md); they do not claim current acceptance of that mode.

**Status:** Accepted for implementation  
**Decision:** Business data is physically separated per Holding into one opaque authoritative write database, one replaceable read database, and one object-storage namespace. A Holding chooses `ErpxManaged` or the opt-in `CustomerDataPlane` mode. In either mode the Holding remains the physical isolation boundary and its business units are logical contexts inside the same database pair. The control plane stores only opaque database/storage/key references and redacted health metadata. Holding credentials never enter a browser, client, customer-facing response, log, support diagnostic, or the ERPX broker. Opaque `HoldingId` remains canonical for storage, queues, audit, bindings, and infrastructure; ADR-023 defines public routing.

**Rationale:** A Holding is the isolation boundary for business aggregates. Database-per-Holding limits blast radius and makes recovery independently testable. Opaque provider bindings avoid leaking organization names or IDs in routing paths.

**Consequences:** ERPX-managed provisioning creates and migrates the database pair, configures RLS and roles, registers the storage namespace, and verifies connectivity. Customer-hosted provisioning validates equivalent resources through the outbound-only data-plane agent and local secret provider. Cross-Holding work must use platform-authorized projections or an audited read gateway; no credential fan-out is allowed. Browser clients never select a physical database route; the server resolves it from the authenticated `AuthorityScope` and registry.

**Evidence required:** isolation integration tests for Holding/Company/Branch scope and both deployment modes, credential-redaction tests, wrong-Holding agent tests, provisioning/restore tests, and Holding-visible global-read audit records.
