# ADR-005: Audited global reads

> **Next.js era: Reconfirmed (2026-09-02) — still authoritative**
> See `CUTOVER.md` (2026-09-02) and `ADR-020-nextjs-stack.md`.


**Status:** Accepted  
**Decision:** Platform global reads use a read gateway. Every detailed customer/module/scope read records actor, customer, module, scope, purpose, timestamp, correlation ID, and outcome. The audit is projected to the customer security view. Bulk export and sensitive document download require explicit approved support/customer authorization.

**Rationale:** Administrators need operational visibility without receiving holding credentials or invisible access to customer data.

**Consequences:** List and detail queries have distinct audit semantics; secrets, credentials, payment instruments, tokens, and unrelated business data are excluded from projections and diagnostics.

**Evidence required:** gateway enforcement, customer-visible audit query, purpose/correlation assertions, secret-redaction tests, and export/download approval tests.