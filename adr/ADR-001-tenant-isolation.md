# ADR-001: Tenant isolation

**Status:** Accepted for implementation  
**Decision:** Tenant business data is physically separated per holding into one opaque authoritative write database, one replaceable read database, and one object-storage namespace. A holding chooses `ErpxManaged` or the opt-in `CustomerDataPlane` mode. In either mode the holding remains the physical isolation boundary and its 12 business units are logical contexts inside the same database pair. The control plane stores only opaque database/storage/key references and redacted health metadata. Tenant credentials never enter browser, Dioxus client, customer-facing responses, logs, support diagnostics, or the ERPX broker.

**Rationale:** A holding is the isolation boundary for business aggregates. Database-per-holding limits blast radius and makes tenant recovery independently testable. Opaque identifiers avoid leaking customer names or IDs in routing paths.

**Consequences:** ERPX-managed provisioning creates and migrates the database pair, configures RLS and roles, registers the storage namespace, and verifies connectivity. Customer-hosted provisioning validates equivalent resources through the outbound-only data-plane agent and local secret provider. Cross-tenant work must use platform-authorized projections or an audited read gateway; no tenant credential fan-out is allowed. Existing tenants remain ERPX-managed until an explicit approved cutover.

**Evidence required:** isolation integration tests for holding/company/branch scope and both deployment modes, credential-redaction tests, wrong-tenant agent tests, provisioning/restore tests, and customer-visible global-read audit records.
