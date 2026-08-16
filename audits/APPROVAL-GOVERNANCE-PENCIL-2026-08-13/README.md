# Approval governance Pencil expansion

Date: 2026-08-13

Artifact: `Designed/MainDesign.pen`

## Scope

This Pencil-only expansion adds approver work, gate drill-down, Sales record detail, and System & access approval governance. No Rust, API, schema, migration, or application-code changes are included.

## Added route screens

- `/operations/approvals/my-work`
- `/operations/approvals/:id/gates/:gateId`
- `/sales/quotes/:id`
- `/sales/revenue-arrangements/:id`
- `/system/approvals`
- `/system/approvals/policies`
- `/system/approvals/policies/:id`
- `/system/approvals/policies/:id/simulate`
- `/system/approvals/roles`
- `/system/approvals/delegations`
- `/system/approvals/escalations`
- `/system/approvals/audit`

Additional prototype-state frames cover decision outcomes, delegation, escalation, expiry, restricted evidence, maker-checker failure, deep-link entry, and source-version invalidation.

## Added reusable components

- Approval Work Item
- Gate Detail Panel
- Evidence Item
- Decision Composer
- Version Comparison
- Audit Event
- Deadline and Escalation Indicator
- Delegation Banner
- Approval Policy Card
- Policy Version Badge
- Condition Builder Row
- Gate Builder Node
- Approver Rule
- SLA Escalation Step
- Simulation Result
- Publish Checklist

The expansion also reuses the existing ERPX button, badge, permission-safe reference, picker, backlinks, security banner, approval gate, milestone, participant decision, session summary, and session-status components.

## Covered scenarios

- Siam Meridian commercial journey from Marketing qualification through quote, parallel Sales/Finance approval, order release, fulfillment exception, Finance posting, cash, revenue, and attribution.
- Rejection, request changes, expiry, delegation, escalation, notification, maker-checker failure, restricted/unavailable evidence, and stale source versions.
- Draft, review requested, changes requested, approved, scheduled, active, superseded, and archived policy states.
- Policy authoring through simulation, independent review, effective-date scheduling, activation, and superseded-version history.
- Session pinning: `@approval-2026-0418` retains policy v6 and quote v4; newly active policy versions affect only new sessions.

## Evidence

- `evidence/operational-contact-sheet.png`
- `evidence/sales-contact-sheet.png`
- `evidence/system-contact-sheet.png`
- `evidence/prototype-map.png`
- Individual full-size screen exports under `evidence/operational/`, `evidence/sales/`, and `evidence/system/`.

## Verification notes

- Pages follow the established ERPX canvas positioning standard: main routes at `x=600`, direct sub-routes at `x=760`, details at `x=920`, deeper routes at `x=1080`, and states 1220px to the right of their related page. Multiple states are stacked in the right-hand state column before the next route group. Routes inside a URL family use 120px vertical spacing; Operations, Sales, and System URL families use the established 540px category gap.
- All new screen frames use the existing ERPX desktop shell and were visually inspected after composition.
- Tall detail/editor frames were expanded where required to prevent content clipping.
- Reference focus, restricted, unavailable, and permission-safe states are represented explicitly.
- Primary buttons, reference chips, open actions, breadcrumbs, and admin links expose their intended target paths visually; `ERPX Approval Prototype Map` records the full transition graph.
- The current Pencil MCP execution surface did not persist `href` interaction properties during verification, so the route graph is documented in-canvas rather than being executable as clickable hotspots.
- Cargo verification was not run because the change is limited to Pencil and retained PNG evidence.
