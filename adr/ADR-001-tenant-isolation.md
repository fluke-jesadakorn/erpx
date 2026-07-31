# ADR-001: Tenant isolation

**Status:** Accepted for implementation  
**Decision:** Tenant business data is physically separated per holding into opaque `tenant_<opaque>_write` and `tenant_<opaque>_read` PostgreSQL databases. The control plane stores only opaque database/storage/key references and health metadata. Tenant credentials never enter browser, Dioxus client, customer-facing responses, logs, or support diagnostics.

**Rationale:** A holding is the isolation boundary for business aggregates. Database-per-holding limits blast radius and makes tenant recovery independently testable. Opaque identifiers avoid leaking customer names or IDs in routing paths.

**Consequences:** Provisioning must create and migrate two databases, configure RLS and roles, register a MinIO namespace, and verify connectivity. Cross-tenant work must use platform-authorized projections or an audited read gateway; no tenant credential fan-out is allowed.

**Evidence required:** isolation integration tests for holding/company/branch scope, credential-redaction tests, provisioning/restore tests, and customer-visible global-read audit records.
