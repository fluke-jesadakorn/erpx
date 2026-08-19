# ADR-001: Holding isolation

**Status:** Accepted for implementation  
**Decision:** Business data is physically separated per Holding into one opaque authoritative write database, one replaceable read database, and one object-storage namespace. A Holding chooses `ErpxManaged` or the opt-in `CustomerDataPlane` mode. In either mode the Holding remains the physical isolation boundary and its business units are logical contexts inside the same database pair. The control plane stores only opaque database/storage/key references and redacted health metadata. Holding credentials never enter a browser, Dioxus client, customer-facing response, log, support diagnostic, or the ERPX broker. `HoldingId` is the canonical organization-root identifier throughout routing, authorization, storage, and infrastructure.

**Rationale:** A Holding is the isolation boundary for business aggregates. Database-per-Holding limits blast radius and makes recovery independently testable. Opaque provider bindings avoid leaking organization names or IDs in routing paths.

**Consequences:** ERPX-managed provisioning creates and migrates the database pair, configures RLS and roles, registers the storage namespace, and verifies connectivity. Customer-hosted provisioning validates equivalent resources through the outbound-only data-plane agent and local secret provider. Cross-Holding work must use platform-authorized projections or an audited read gateway; no credential fan-out is allowed. Browser clients never select a physical database route; the server resolves it from the authenticated `AuthorityScope` and registry.

**Evidence required:** isolation integration tests for Holding/Company/Branch scope and both deployment modes, credential-redaction tests, wrong-Holding agent tests, provisioning/restore tests, and Holding-visible global-read audit records.
