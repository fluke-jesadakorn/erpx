# ADR-008: Operations context boundaries

> **Next.js era: Reconfirmed (2026-09-02) — still authoritative**
> See `CUTOVER.md` (2026-09-02) and `ADR-020-nextjs-stack.md`.


**Status:** Accepted  
**Decision:** Operations owns orchestration projections, plans, commitments, fulfillment/capacity scenarios, exceptions, work queues, calendars, shifts, handovers, SOPs, dispatch batches, and escalation. It consumes versioned events from Sales, Procurement, Inventory, Finance, Maintenance, Support, and Integrations. It never becomes the source of sales orders, purchase orders, stock movements, invoices, or journals.

**Rationale:** Cross-functional visibility and branch execution need a coherent read model without duplicating financial or transactional authority.

**Consequences:** Each source context publishes stable events; Operations projections are idempotent and branch-scoped. Legacy dashboard totals are reconciliation fixtures during migration, not a second source of truth.

**Evidence required:** event ownership map, duplicate/replay tests, branch isolation, source-to-dashboard reconciliation, exception recovery, and Legacy/ERPX totals comparison.

## Workforce scheduling amendment (2026-09-17)

People owns staff roster weeks, work shifts, shift templates, assignments, and staff change requests. Managers plan and publish the weekly department roster; staff may request open slots or exchanges. A swap requires coworker acceptance followed by an independent, scoped roster manager. Operations may consume this schedule for coverage planning, without maintaining a second schedule. This supersedes the general reference to Operations owning “shifts” above for workforce scheduling.

POS continues to own **Cash sessions**, including opening float, cash responsibility, closing counts, and independent review. The UI name distinguishes register accountability from planned staff work. Workforce scheduling neither opens a register nor proves attendance or payroll entitlement.
