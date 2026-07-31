# ADR-009: Marketing and CRM boundaries

**Status:** Accepted  
**Decision:** Marketing owns acquisition profiles, consent, suppression, audiences, campaigns, journeys, content, channels, and attribution. CRM owns accepted leads, accounts, contacts, opportunities, activities, qualification, and conversion. Consent is checked both when scheduling and immediately before delivery. Approved n8n actions are explicit integration commands, not arbitrary database access.

**Rationale:** Consent and delivery compliance must be enforced at the last responsible moment, while qualified revenue ownership belongs to CRM and Finance reconciliation.

**Consequences:** Suppression cancels queued sends; message/lead retries are idempotent; custom domains and MinIO objects are tenant-bound; attribution models emit Finance-reconcilable revenue facts.

**Evidence required:** consent/suppression/erasure tests, journey resume/idempotency, tenant-domain isolation, n8n allowlist enforcement, attribution model fixtures, and ROAS/Finance reconciliation.
