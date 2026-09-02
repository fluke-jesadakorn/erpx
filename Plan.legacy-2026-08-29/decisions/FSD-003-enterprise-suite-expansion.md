# FSD-003 — Enterprise Suite Expansion

- Status: Accepted
- Owner: Product Council
- Effective baseline: 1.0.0
- Supersedes: the twelve-workspace boundary as a future target

## Decision

ERPX expands from 21 current contexts and 12 workspaces with eight paired
contexts/workspaces: Manufacturing, Supply & Demand Planning, Projects,
E-commerce, Recruiting & Talent, Learning, Benefits, and Field Service.

Manufacturing separates discrete and process models. Planning provides MRP,
DRP, and capacity scenarios with human release. Projects supports services,
internal, and capital work through a hierarchical work graph. Commerce is
headless through ERPX-controlled adapters. Talent covers recruiting,
performance, succession, and compensation proposals. Learning records
training completion. Benefits hands approved deductions to Payroll. Field
Service covers service order through signed completion and billing handoff.

## Lifecycle rule

All 21 current contexts are target lifecycle-complete: configuration, creation,
change, approval, activation, execution, correction, cancellation, closure,
archive, allowed reversal/return, handoff, reconciliation, recovery, audit,
privacy, and retention must be explicit. This is lifecycle completeness rather
than vendor-feature parity.
