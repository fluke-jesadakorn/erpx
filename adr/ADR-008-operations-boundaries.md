# ADR-008: Operations context boundaries

**Status:** Accepted  
**Decision:** Operations owns orchestration projections, plans, commitments, fulfillment/capacity scenarios, exceptions, work queues, calendars, shifts, handovers, SOPs, dispatch batches, and escalation. It consumes versioned events from Sales, Procurement, Inventory, Finance, Maintenance, Support, and Integrations. It never becomes the source of sales orders, purchase orders, stock movements, invoices, or journals.

**Rationale:** Cross-functional visibility and branch execution need a coherent read model without duplicating financial or transactional authority.

**Consequences:** Each source context publishes stable events; Operations projections are idempotent and branch-scoped. Legacy dashboard totals are reconciliation fixtures during migration, not a second source of truth.

**Evidence required:** event ownership map, duplicate/replay tests, branch isolation, source-to-dashboard reconciliation, exception recovery, and Legacy/ERPX totals comparison.
