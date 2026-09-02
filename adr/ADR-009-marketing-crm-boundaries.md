# ADR-009: Marketing and CRM boundaries

**Status:** Accepted (n8n retired by ADR-015)  
**Decision:** Marketing owns acquisition profiles, consent, suppression, audiences, campaigns, journeys, content, channels, and attribution. CRM owns accepted leads, accounts, contacts, opportunities, activities, qualification, and conversion. Consent is checked both when scheduling and immediately before delivery. Approved integration commands are explicit, not arbitrary database access.

**Rationale:** Consent and delivery compliance must be enforced at the last responsible moment, while qualified revenue ownership belongs to CRM and Finance reconciliation.

**Consequences:** Suppression cancels queued sends; message/lead retries are idempotent; custom domains and MinIO objects are holding-bound; attribution models emit Finance-reconcilable revenue facts.

**Evidence required:** consent/suppression/erasure tests, journey resume/idempotency, holding-domain isolation, integration allowlist enforcement, attribution model fixtures, and ROAS/Finance reconciliation.
