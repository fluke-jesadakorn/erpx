# ADR-007: Stripe authority for commercial state

**Status:** Accepted  
**Decision:** Stripe is authoritative for payment/subscription lifecycle. ERPX owns catalog revisions, mapping, entitlements, allocation, grandfathering decisions, and local reconciliation metadata. Stripe webhooks are verified over the raw body, persisted in an inbox, processed idempotently and out-of-order safely, and reconciled by workers.

**Rationale:** Local billing state must not override payment provider truth while still preserving product capacity and holding allocation rules.

**Consequences:** The adapter is pinned to an official API/OpenAPI revision and Stripe API version. ERPX keeps a seven-day default grace policy and read-only capability set for payment failure; recovery re-enables only currently authorized mutations.

**Evidence required:** duplicate/out-of-order webhook tests, signature tests, Test Clock scenarios, catalog immutability/grandfathering, capacity allocation, reconciliation, and recovery tests.
