# Supply Chain, Assets, and Operations Requirements

## Portfolio boundary

Procurement owns purchase orders; Inventory owns stock and reservations; Assets
owns asset lifecycle; Maintenance owns work orders and downtime; Quality owns
inspections and release disposition; Operations owns cross-domain exception
and work-queue orchestration but never the source transaction.

## Procurement

### REQ-PRC-001 — Purchase-order lifecycle

**Requirement.** ERPX shall manage purchase orders through creation, approval
where required, release, and cancellation with versioned supplier and line
commitments.

**Rationale.** Supplier commitment needs a controlled source before inventory
or finance acts.

**Acceptance criteria.** Lines identify item, quantity, price, currency, and
scope; release requires current version, authority, and approval; released
terms cannot be silently edited; cancellation respects receipt and payable
state.

**Sources.** [Procurement context](../../crates/contexts/procurement/src/lib.rs),
[Procurement API](../../crates/proto/proto/erpx/procurement/v2/service.proto).

### REQ-PRC-002 — Purchase approval and budget control

**Requirement.** ERPX shall evaluate purchase release against approval policy,
actor limit, budget, entitlement, and branch scope.

**Rationale.** Purchasing authority is more than permission to edit a purchase
order.

**Acceptance criteria.** Maker-checker applies at configured thresholds;
reserved budget references the purchase order and version; a stale or expired
approval cannot release changed terms; denied release leaves no supplier,
inventory, or payable side effect.

**Sources.** [Procurement context](../../crates/contexts/procurement/src/lib.rs),
[approval governance](../04-approval-governance.md),
[finance budget](../../crates/domain/src/finance_budget.rs).

### REQ-PRC-003 — Released procurement facts

**Requirement.** ERPX shall publish released and cancelled purchase-order facts
for Inventory, Finance, Operations, and Quality through versioned contracts.

**Rationale.** Receiving, payable, planning, and inspection depend on the same
approved commitment.

**Acceptance criteria.** Events identify order, lines, supplier, scope,
version, and status; duplicate delivery is safe; consumers reject an unknown
or incompatible version; Procurement remains the only order mutation owner.

**Sources.** [Procurement context](../../crates/contexts/procurement/src/lib.rs),
[procurement integration](../../crates/contexts/procurement/src/integration/mod.rs),
[integration contracts](../../crates/integration-contracts/src/lib.rs).

## Inventory

### REQ-INV-001 — Stock-item authority

**Requirement.** ERPX shall register stock items by warehouse and physical
scope with a stable identity and authorized lifecycle.

**Rationale.** Availability calculations require an authoritative stock record.

**Acceptance criteria.** Item and warehouse identity are unique in scope;
registration is idempotent; stock state cannot be created by a read projection;
cross-branch access follows active physical routing and authorization.

**Sources.** [Inventory context](../../crates/contexts/inventory/src/lib.rs),
[Inventory API](../../crates/proto/proto/erpx/inventory/v2/service.proto).

### REQ-INV-002 — Traceable stock movements

**Requirement.** ERPX shall change on-hand stock only through traceable receive,
issue, return, adjustment, sale, refund, or other approved movement facts.

**Rationale.** A mutable balance without movement evidence cannot be audited or
reconciled.

**Acceptance criteria.** Each movement has unique identity, quantity, source,
actor or source event, and time; duplicates are idempotent; stock cannot fall
below policy-defined limits; on-hand equals the ordered movement history.

**Sources.** [Inventory context](../../crates/contexts/inventory/src/lib.rs),
[inventory domain](../../crates/contexts/inventory/src/domain/mod.rs).

### REQ-INV-003 — Inventory reservations

**Requirement.** ERPX shall reserve, release, and consume available inventory
against a specific source demand without transferring source-order ownership.

**Rationale.** Sales and Operations need reliable availability while Inventory
retains stock authority.

**Acceptance criteria.** Reservation identifies source, item, warehouse,
quantity, validity, and status; active reservations cannot exceed available
policy; release is idempotent; consumption references fulfillment; expired or
cancelled reservations no longer reduce availability.

**Sources.** [Inventory context](../../crates/contexts/inventory/src/lib.rs),
[procurement and inventory domain](../../crates/domain/src/procurement_inventory.rs).

### REQ-INV-004 — Inventory reconciliation

**Requirement.** ERPX shall reconcile on-hand, reserved, available, received,
issued, sales, refund, and procurement facts by item, warehouse, branch, and
source position.

**Rationale.** Cross-domain failures must be visible before they become hidden
availability errors.

**Acceptance criteria.** Rebuild from the same accepted movement facts yields
the same balances; mismatches identify source and quantity; stale projections
are labeled; correction uses an authorized movement rather than direct balance
editing.

**Sources.** [Inventory context](../../crates/contexts/inventory/src/lib.rs),
[SCM projection](../../apps/worker/src/scm_projection.rs).

## Assets and Maintenance

### REQ-AST-001 — Asset lifecycle

**Requirement.** ERPX shall manage assets through creation, placement in
service, degradation, maintenance, return to service, and retirement.

**Rationale.** Asset availability and financial/operational decisions depend on
an explicit state.

**Acceptance criteria.** Asset identity and scope are stable; invalid lifecycle
transitions are rejected; retirement is terminal except for an explicitly
defined correction; state changes publish versioned events.

**Sources.** [Assets context](../../crates/contexts/assets/src/lib.rs),
[Assets API](../../crates/proto/proto/erpx/assets/v2/service.proto).

### REQ-AST-002 — Asset-to-maintenance linkage

**Requirement.** ERPX shall link maintenance requests and work orders to an
authoritative asset version without allowing Maintenance to redefine the
asset.

**Rationale.** Work execution and asset registry have different ownership.

**Acceptance criteria.** A work order references a valid asset and scope;
maintenance start can produce an approved asset-state fact; completion does not
return an asset to service unless the asset lifecycle accepts it; replay is
idempotent.

**Sources.** [Assets context](../../crates/contexts/assets/src/lib.rs),
[Maintenance context](../../crates/contexts/maintenance/src/lib.rs),
[asset-maintenance API](../../crates/proto/proto/erpx/maintenance/v2/service.proto).

### REQ-AST-003 — Reliability and availability evidence

**Requirement.** ERPX shall derive asset reliability and availability measures
from traceable lifecycle, downtime, and maintenance facts.

**Rationale.** Reliability indicators must be reproducible rather than manually
entered dashboard values.

**Acceptance criteria.** Measures identify asset, period, source facts, and
calculation version; negative duration and impossible percentages are rejected;
stale or incomplete source coverage is visible; recalculation is deterministic.

**Sources.** [Asset-maintenance domain](../../crates/domain/src/asset_maintenance.rs),
[Assets context](../../crates/contexts/assets/src/lib.rs).

### REQ-MNT-001 — Work-order lifecycle

**Requirement.** ERPX shall manage maintenance work orders through open,
schedule, start, block, resume, complete, and cancel states.

**Rationale.** Maintenance work requires durable ownership, status, and
recovery from interruptions.

**Acceptance criteria.** Scheduling identifies scope, asset, owner, planned
time, and version; blocked work records reason; completion requires started
work and result evidence; cancelled or completed work rejects further
operational transitions.

**Sources.** [Maintenance context](../../crates/contexts/maintenance/src/lib.rs),
[Maintenance API](../../crates/proto/proto/erpx/maintenance/v2/service.proto).

### REQ-MNT-002 — Downtime recording

**Requirement.** ERPX shall record bounded asset downtime against an active
maintenance work order and shall prevent overlapping or negative intervals.

**Rationale.** Operational availability and service impact rely on accurate
downtime.

**Acceptance criteria.** Downtime records start, end, reason, asset, order, and
scope; end is after start; invalid overlap is rejected under policy; corrected
records preserve prior audit evidence.

**Sources.** [Maintenance context](../../crates/contexts/maintenance/src/lib.rs),
[maintenance domain](../../crates/contexts/maintenance/src/domain/mod.rs).

### REQ-MNT-003 — Maintenance queue and planning

**Requirement.** ERPX shall provide authorized maintenance lists by status,
asset, scope, assignee, and schedule with projection freshness.

**Rationale.** Planners need actionable work without reading authoritative
write tables.

**Acceptance criteria.** Filters are bounded and scope-authorized; list and
detail results identify freshness; denied rows are not inferred from counts;
read-model rebuild preserves business ordering and identity.

**Sources.** [Maintenance context](../../crates/contexts/maintenance/src/lib.rs),
[Maintenance API](../../crates/proto/proto/erpx/maintenance/v2/service.proto).

## Quality

### REQ-QUA-001 — Inspection lifecycle

**Requirement.** ERPX shall manage quality inspections through creation,
release, start, result recording, and cancellation for approved inspection
kinds.

**Rationale.** A quality disposition must be tied to a controlled inspection
record.

**Acceptance criteria.** Inspection references source, kind, scope, subject,
and version; start requires released state; result cannot be recorded twice as
different outcomes; terminal inspections reject invalid transitions.

**Sources.** [Quality context](../../crates/contexts/quality/src/lib.rs),
[Quality API](../../crates/proto/proto/erpx/quality/v2/service.proto).

### REQ-QUA-002 — Quality result and disposition

**Requirement.** ERPX shall record quality result, evidence, and release or
hold disposition without directly mutating Procurement, Inventory, Sales, or
Assets.

**Rationale.** Quality controls downstream eligibility while source ownership
remains intact.

**Acceptance criteria.** Result records measured facts and actor; disposition
uses a versioned integration fact; consumers apply the fact idempotently;
failed or cancelled inspection cannot be represented as passed.

**Sources.** [Quality context](../../crates/contexts/quality/src/lib.rs),
[quality integration](../../crates/contexts/quality/src/integration/mod.rs).

### REQ-QUA-003 — Quality reconciliation

**Requirement.** ERPX shall reconcile inspection subject, result, disposition,
and consumer application by source position.

**Rationale.** A released item or asset must be explainable from authoritative
quality evidence.

**Acceptance criteria.** Missing, duplicate, or out-of-order facts are visible;
projection rebuild is deterministic; reconciliation does not manufacture a
result; unresolved mismatches block any completion claim that depends on the
inspection.

**Sources.** [Quality context](../../crates/contexts/quality/src/lib.rs),
[quality projection](../../apps/worker/src/quality_projection.rs).

## Operations orchestration

### REQ-OPS-001 — Source ownership preservation

**Requirement.** ERPX shall make Operations consume versioned source events and
shall not become authority for sales orders, purchase orders, stock movements,
invoices, journals, assets, or support tickets.

**Rationale.** Cross-functional coordination must not create a competing
transaction system.

**Acceptance criteria.** Operations records source database, source identity,
position, type, and version; actions on an exception cannot mutate its source
record directly; source correction arrives as a new versioned event.

**Sources.** [Operations decision](../../adr/ADR-008-operations-boundaries.md),
[Operations context](../../crates/contexts/operations/src/lib.rs).

### REQ-OPS-002 — Idempotent exception detection

**Requirement.** ERPX shall detect operational exceptions from source facts by
versioned policy and shall suppress duplicate exceptions for the same policy
and source condition.

**Rationale.** Replayed events must not multiply operational workload.

**Acceptance criteria.** Detection records policy version and source position;
duplicates return the existing outcome; corrected source facts can resolve or
supersede the condition; policy failure leaves source work unchanged.

**Sources.** [Operations context](../../crates/contexts/operations/src/lib.rs),
[operations source worker](../../apps/worker/src/contexts/operations_source.rs).

### REQ-OPS-003 — Exception lifecycle

**Requirement.** ERPX shall manage exceptions through acknowledge, reassign,
escalate, resolve, and close states with explicit reason and evidence.

**Rationale.** Cross-domain issues require visible ownership and closure
criteria.

**Acceptance criteria.** Transitions validate current state and version;
resolution records the source or compensating evidence; closure requires a
resolved condition when policy says so; history remains append-only.

**Sources.** [Operations context](../../crates/contexts/operations/src/lib.rs),
[Operations API](../../crates/proto/proto/erpx/operations/v2/service.proto).

### REQ-OPS-004 — Governed assignment

**Requirement.** ERPX shall assign operational work using current scope,
role, manager-chain, workload, and work-queue policy without granting new
business authority.

**Rationale.** Assignment directs eligible work; it must not bypass
authorization.

**Acceptance criteria.** Assignee eligibility is evaluated at assignment and
action time; expired or revoked roles trigger reassignment; manager fallback
remains scope-contained; assignment history identifies policy and actor.

**Sources.** [Operations context](../../crates/contexts/operations/src/lib.rs),
[operations completion](../../crates/domain/src/operations_completion.rs),
[Operations projection runtime](../../apps/worker/src/contexts/operations.rs).

### REQ-OPS-005 — Versioned work-queue policy

**Requirement.** ERPX shall manage work-queue policies through draft,
publication, supersession, and read-only history.

**Rationale.** Queue routing must be predictable and reproducible.

**Acceptance criteria.** Published policy is immutable; publication is
version-checked and authorized; new source facts use the applicable published
version; existing assignments retain the policy evidence that selected them.

**Sources.** [Operations context](../../crates/contexts/operations/src/lib.rs),
[Operations API](../../crates/proto/proto/erpx/operations/v2/service.proto).

### REQ-OPS-006 — SLA, notification, and escalation

**Requirement.** ERPX shall attach bounded due and escalation policy to
operational work and shall issue durable, idempotent notifications.

**Rationale.** Critical work must surface before and after breach without
duplicating alerts.

**Acceptance criteria.** SLA cannot exceed the approved seven-day bound;
escalation does not occur before due time; notification identity prevents
duplicates after restart; breach and escalation remain visible in the queue and
audit.

**Sources.** [Operations context](../../crates/contexts/operations/src/lib.rs),
[operations domain](../../crates/domain/src/operations.rs),
[Operations durable command/outbox adapter](../../crates/adapters/operations-sqlx/src/command.rs).

### REQ-OPS-007 — Approval work coordination

**Requirement.** ERPX shall make Operations provide approver work, gate detail,
decision, delegation, escalation, and audit views while the source context owns
the approved record.

**Rationale.** Cross-domain approvals need one work experience without moving
record authority.

**Acceptance criteria.** Work items link to exact source and policy versions;
decisions use the shared approval requirements; deep links preserve scope and
authorization; completion publishes an outcome fact that the source context
must independently validate.

**Sources.** [Approval requirements](../04-approval-governance.md),
[approval design](../../audits/APPROVAL-GOVERNANCE-PENCIL-2026-08-13/README.md),
[Operations context](../../crates/contexts/operations/src/lib.rs).

### REQ-OPS-008 — Operational summary and reconciliation

**Requirement.** ERPX shall provide branch-scoped operational summaries from
replaceable projections and shall reconcile them to source-context positions.

**Rationale.** Dashboards must reflect source facts rather than become manually
maintained totals.

**Acceptance criteria.** Summary identifies freshness and source counts;
duplicate or missing events are visible; rebuild yields the same result;
holding consolidation preserves branch provenance; unresolved differences are
not reported as complete.

**Sources.** [Operations decision](../../adr/ADR-008-operations-boundaries.md),
[Operations context](../../crates/contexts/operations/src/lib.rs),
[Operations projection and rebuild runtime](../../apps/worker/src/contexts/operations.rs).

## Lifecycle-completion requirements

| ID | Title | Normative requirement | Rationale | Acceptance paths | Source | Scenarios |
|---|---|---|---|---|---|---|
| REQ-PRC-090 | Procurement lifecycle coverage | ERPX shall govern supplier request, sourcing, requisition, approval, order, change, receipt, return, invoice-match, cancellation, close, and archive. | Procure-to-pay needs complete states. | Success: valid transition links source versions; Denial: unauthorized order/change is rejected; Failure: partial receipt/match remains reconcilable. | [FSD-003](../decisions/FSD-003-enterprise-suite-expansion.md) | E2E-07 |
| REQ-PRC-091 | Procurement reconciliation | ERPX shall reconcile supplier, budget, order, inventory receipt, quality, invoice, payment, return, and planning facts. | Procurement crosses owners. | Success: each source outcome converges; Denial: Procurement cannot post stock/journal; Failure: mismatch creates governed exception. | [FSD-003](../decisions/FSD-003-enterprise-suite-expansion.md) | E2E-07, E2E-34 |
| REQ-INV-090 | Inventory lifecycle coverage | ERPX shall govern item/site setup, reservation, issue, receipt, transfer, count, adjustment, return, quarantine, release, close, and archive. | Stock authority needs explicit transitions. | Success: movement balances fixed-precision quantity; Denial: negative/unauthorized movement is rejected; Failure: partial movement rolls back. | [FSD-003](../decisions/FSD-003-enterprise-suite-expansion.md) | E2E-07, E2E-08 |
| REQ-INV-091 | Inventory reconciliation | ERPX shall reconcile physical stock, reservations, lots/serials, quality, manufacturing, fulfillment, field service, and Finance facts. | Inventory truth serves many domains. | Success: difference is explained or corrected; Denial: consumer cannot edit stock; Failure: unresolved variance blocks governed close. | [FSD-003](../decisions/FSD-003-enterprise-suite-expansion.md) | E2E-08, E2E-32, E2E-40 |
| REQ-AST-090 | Asset lifecycle coverage | ERPX shall govern proposal, acquisition, capitalization, activation, transfer, impairment, maintenance linkage, disposal, derecognition, and archive. | Asset accounting and operations need one lifecycle. | Success: valid transition links Finance facts; Denial: unauthorized disposal is rejected; Failure: partial capitalization remains pending. | [FSD-003](../decisions/FSD-003-enterprise-suite-expansion.md) | E2E-09, E2E-35 |
| REQ-AST-091 | Asset reconciliation | ERPX shall reconcile asset register, project capitalization, inventory components, maintenance, meters, depreciation, and Finance. | Asset records can drift from accounting/operations. | Success: source facts agree by period; Denial: Assets cannot fabricate journal; Failure: mismatch blocks close/disposal. | [FSD-003](../decisions/FSD-003-enterprise-suite-expansion.md) | E2E-09, E2E-35 |
| REQ-MNT-090 | Maintenance lifecycle coverage | ERPX shall govern request, plan, approve, schedule, release, execute, pause, complete, cancel, close, and archive for maintenance work. | Asset work needs complete control. | Success: valid order transition records evidence; Denial: safety/skill gate blocks execution; Failure: interruption remains resumable. | [FSD-003](../decisions/FSD-003-enterprise-suite-expansion.md) | E2E-09 |
| REQ-MNT-091 | Maintenance reconciliation | ERPX shall reconcile asset availability, parts, labor, downtime, quality, production constraints, cost, and follow-up outcomes. | Maintenance impacts multiple sources. | Success: closed work links accepted facts; Denial: Maintenance cannot edit source stock/journal; Failure: gap blocks close. | [FSD-003](../decisions/FSD-003-enterprise-suite-expansion.md) | E2E-09, E2E-32 |
| REQ-QUA-090 | Quality lifecycle coverage | ERPX shall govern plan, sample, inspection, result, nonconformance, disposition, release, correction, closure, and archive. | Quality decisions need complete evidence. | Success: authorized disposition pins evidence; Denial: source operator cannot self-release when policy forbids; Failure: missing test blocks release. | [FSD-003](../decisions/FSD-003-enterprise-suite-expansion.md) | E2E-07, E2E-33 |
| REQ-QUA-091 | Quality reconciliation | ERPX shall reconcile inspections and dispositions with procurement, inventory, manufacturing, assets, maintenance, returns, and suppliers. | Quality gates source lifecycles. | Success: accepted disposition reaches each source; Denial: Quality cannot alter source quantities directly; Failure: mismatch stays quarantined. | [FSD-003](../decisions/FSD-003-enterprise-suite-expansion.md) | E2E-07, E2E-33 |
| REQ-OPS-090 | Operations lifecycle coverage | ERPX shall govern exception detection, triage, assignment, escalation, resolution, confirmation, closure, reopening, and archive. | Cross-domain exceptions need complete coordination. | Success: valid transition links source; Denial: Operations cannot mutate source aggregate; Failure: recurrence creates/reopens by policy idempotently. | [FSD-003](../decisions/FSD-003-enterprise-suite-expansion.md) | E2E-19, E2E-21 |
| REQ-OPS-091 | Operations reconciliation | ERPX shall reconcile exception state, source correction, SLA, assignment, approval, notifications, and customer confirmation. | Closure must reflect actual outcome. | Success: all facts agree before close; Denial: operator cannot assert source success; Failure: mismatch returns item to work. | [FSD-003](../decisions/FSD-003-enterprise-suite-expansion.md) | E2E-19, E2E-21 |
