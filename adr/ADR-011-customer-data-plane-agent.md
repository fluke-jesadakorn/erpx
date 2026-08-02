# ADR-011: Customer-hosted data plane and outbound agent

**Status:** Accepted for implementation
**Decision:** ERPX remains the managed control plane and console. A holding may opt into a customer-hosted data plane containing its write/read PostgreSQL databases, object storage, directory connectivity, secret provider, background workers, backup integration, and an ERPX data-plane agent. The agent establishes an outbound-only mTLS channel, authenticates with a one-time enrollment token and customer certificate, pins ERPX signing keys, and accepts only signed, versioned desired configuration and a closed set of typed operations.

Configuration precedence is mandatory platform policy, then holding infrastructure configuration, then business-unit specialization. Platform operators may propose changes at every layer, but a customer-hosted change is applied only after customer approval. Optimistic concurrency rejects stale desired versions. Secret values, generic SQL, generic shell, and unrestricted database-owner authority are not representable in the broker contract.

Each holding owns one directory connector boundary and one core database pair. The canonical Operations, Finance, Sales, Procurement, Inventory, Assets, Maintenance, Quality, Marketing & CRM, HR & Payroll, Point of Sale, and Legal units configure logical allocation, allow-listed group-to-role mappings, integrations, retention, projections, workflows, and approval limits. They cannot install independent core connectors or databases.

**Rationale:** Customers can keep regulated business data and credentials inside their network without splitting ERPX authorization, entitlement, audit, or product behavior into a second application stack. Outbound connectivity and capability admission minimize exposed network surface and standing privilege.

**Consequences:** Existing tenants default to `ErpxManaged`. Customer-hosted activation requires enrollment, local resource preflight, migration and recovery validation, customer approval, and observed-state verification. An offline agent leaves local jobs running but denies new brokered business work and terminates active Root sessions. ERPX retains redacted last-known control-plane health and the ability to submit proposals or access requests; the customer retains the right to reject every session.

**Evidence required:** enrollment expiry/replay/wrong-tenant and certificate-rotation tests; mTLS and signature tests; desired-state concurrency, drift, and secret-redaction tests; typed-operation closure tests; local projection/directory/backup/restore/migration tests; customer Root approval/rejection/revocation/expiry tests; identical 12-unit catalog tests; and managed versus customer-agent routing tests.
