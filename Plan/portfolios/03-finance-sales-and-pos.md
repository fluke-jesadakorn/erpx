# Finance, Sales, and Point-of-Sale Requirements

## Portfolio boundary

Sales owns quotes, sales orders, fulfillment state, and revenue arrangements.
Point of Sale owns register sales, payments, and refund decisions. Finance owns
accounting periods, charts, journals, subledgers, invoices, banking, tax, close,
and revenue recognition. Each context publishes facts; none directly mutates
another context's aggregate.

## Finance

### REQ-FIN-001 — Governed chart of accounts

**Requirement.** ERPX shall manage versioned charts of accounts with explicit
account code, category, normal balance, status, and usage controls.

**Rationale.** Journal validity and reporting depend on a stable accounting
structure.

**Acceptance criteria.** Account codes are unique within a chart; balanced
accounting rules use category and normal balance; inactive accounts reject new
posting while retaining history; chart and account changes are versioned and
audited.

**Sources.** [Finance context](../../crates/contexts/finance/src/lib.rs),
[Finance API](../../crates/proto/proto/erpx/finance/v1/service.proto),
[finance chart tests](../../apps/worker/tests/finance_chart_projection.rs).

### REQ-FIN-002 — Balanced journal lifecycle

**Requirement.** ERPX shall create, validate, and post balanced journals with
immutable posted lines and explicit source attribution.

**Rationale.** The general ledger must remain arithmetically correct and
traceable to its business origin.

**Acceptance criteria.** Total debits equal total credits before posting;
accounts and periods are open and active; posted journals cannot be edited or
deleted; correction uses an authorized reversing or follow-up journal with a
source reference.

**Sources.** [Finance context](../../crates/contexts/finance/src/lib.rs),
[Finance API](../../crates/proto/proto/erpx/finance/v1/service.proto).

### REQ-FIN-003 — Accounting period and close

**Requirement.** ERPX shall manage accounting periods through open, closing,
closed, and governed reopen behavior with review evidence.

**Rationale.** Period control is required for reliable financial statements.

**Acceptance criteria.** Close identifies unresolved journals, subledgers,
banking, tax, and review controls; closing blocks disallowed new postings;
closure records approval and evidence versions; reopen requires separate
authority, fresh MFA, reason, and immutable audit.

**Sources.** [Finance context](../../crates/contexts/finance/src/lib.rs),
[finance close domain](../../crates/domain/src/finance_close.rs),
[close projection](../../apps/worker/src/finance_close_projection.rs).

### REQ-FIN-004 — Receivable and payable subledgers

**Requirement.** ERPX shall maintain traceable receivable and payable items
from authorized source facts and shall reconcile their settlement to the
general ledger.

**Rationale.** Customer and supplier obligations need detail without becoming
unlinked ledger totals.

**Acceptance criteria.** Sales invoice facts create receivables and released
procurement facts create payables idempotently; settlement cannot exceed the
open amount; every item references its source and journal; subledger and ledger
balances expose reconciliation differences.

**Sources.** [Finance context](../../crates/contexts/finance/src/lib.rs),
[finance subledgers](../../crates/domain/src/finance_subledgers.rs),
[Finance API](../../crates/proto/proto/erpx/finance/v1/service.proto).

### REQ-FIN-005 — Banking import and matching

**Requirement.** ERPX shall import bank-provider transactions through a typed,
idempotent provider boundary and shall separate proposed matching from
authorized accounting settlement.

**Rationale.** Bank data is external evidence, not permission to create an
unreviewed journal.

**Acceptance criteria.** Import records provider account, cursor, source ID,
amount, currency, date, and response identity; duplicate imports do not create
duplicate facts; match candidates identify confidence and evidence; confirmed
matching requires current authority and produces a traceable settlement.

**Sources.** [Finance banking domain](../../crates/domain/src/finance_banking.rs),
[banking import worker](../../apps/worker/src/finance_banking_import.rs),
[bank import tests](../../apps/worker/tests/finance_banking_import_jobs.rs).

### REQ-FIN-006 — Budget control

**Requirement.** ERPX shall manage versioned budgets and shall evaluate
authorized commitments and spend against the applicable scope, period, and
allocation.

**Rationale.** Budget visibility and approval limits must be based on durable
commitments rather than informal totals.

**Acceptance criteria.** Budget creation identifies currency, period, scope,
and allocations; reservation is idempotent and cannot exceed approved policy;
release and consumption reference the originating request; stale budget
versions cannot be overwritten.

**Sources.** [Finance budget domain](../../crates/domain/src/finance_budget.rs),
[Finance API](../../crates/proto/proto/erpx/finance/v1/service.proto).

### REQ-FIN-007 — Customer invoicing

**Requirement.** ERPX shall create, post, and cancel customer invoices from
authorized billing or sales facts while maintaining a single invoice authority
in Finance.

**Rationale.** Commercial billing events and accounting invoices have related
but distinct ownership.

**Acceptance criteria.** Invoice creation is idempotent by source event;
posting validates period, customer, currency, amounts, and accounts;
cancellation follows status and accounting rules; invoice projection exposes
source and resulting journal or receivable references.

**Sources.** [Finance context](../../crates/contexts/finance/src/lib.rs),
[Finance API](../../crates/proto/proto/erpx/finance/v1/service.proto).

### REQ-FIN-008 — Revenue recognition

**Requirement.** ERPX shall create revenue schedules from activated Sales
revenue arrangements and recognize revenue only from accepted fulfillment
facts under the applicable obligation rules.

**Rationale.** Contract value, invoicing, cash, and recognized revenue are not
the same event.

**Acceptance criteria.** A schedule references the immutable arrangement and
obligations; duplicate fulfillment facts are ignored; recognized amount cannot
exceed the allocated obligation; cancellation and completion follow explicit
states; every recognition entry maps to a journal.

**Sources.** [Finance context](../../crates/contexts/finance/src/lib.rs),
[Sales context](../../crates/contexts/sales/src/lib.rs),
[Finance API](../../crates/proto/proto/erpx/finance/v1/service.proto).

### REQ-FIN-009 — Statutory tax reporting

**Requirement.** ERPX shall produce statutory tax reports from authoritative,
versioned accounting facts and shall preserve jurisdiction, tax kind, source,
and reporting-period traceability.

**Rationale.** Tax reports must be reproducible and reconcilable to posted
business records.

**Acceptance criteria.** Only posted eligible facts enter a report; report
totals map to source invoice or journal facts; corrections preserve prior
versions; Thai statutory tax kinds and evidence are supported without making a
projection the posting authority.

**Sources.** [Finance context](../../crates/contexts/finance/src/lib.rs),
[finance tax domain](../../crates/domain/src/finance_tax.rs),
[Finance API](../../crates/proto/proto/erpx/finance/v1/service.proto).

### REQ-FIN-010 — Financial reconciliation

**Requirement.** ERPX shall expose reconciliation among source transactions,
subledgers, bank facts, journals, trial balances, tax, close, and revenue
schedules.

**Rationale.** Stakeholders need evidence that cross-domain processing has not
lost or duplicated value.

**Acceptance criteria.** Reconciliation identifies counts, amounts, source
positions, and bounded differences; mismatches cannot be reported as complete;
rebuild produces the same totals from the same source facts; consolidated views
retain holding and branch provenance.

**Sources.** [Finance projection](../../apps/worker/src/finance_projection.rs),
[finance completion runtime](../../apps/worker/src/finance_completion_runtime.rs),
[Finance context](../../crates/contexts/finance/src/lib.rs).

## Sales

### REQ-SAL-001 — Quote lifecycle

**Requirement.** ERPX shall manage quotes through creation, send, approval when
required, acceptance, rejection, and expiry with versioned commercial terms.

**Rationale.** A customer commitment must reference the exact offer that was
reviewed and accepted.

**Acceptance criteria.** A sent quote cannot be silently edited; acceptance
uses the current valid version and customer authority; expired or rejected
quotes cannot be accepted; approval gates pin the quote version and invalidate
when material terms change.

**Sources.** [Sales context](../../crates/contexts/sales/src/lib.rs),
[Sales API](../../crates/proto/proto/erpx/sales/v1/service.proto),
[approval design](../../audits/APPROVAL-GOVERNANCE-PENCIL-2026-08-13/README.md).

### REQ-SAL-002 — Sales order lifecycle

**Requirement.** ERPX shall manage sales orders through creation,
confirmation, partial fulfillment, fulfillment, cancellation, and completion
without delegating order authority to Operations.

**Rationale.** Order commitment remains a Sales responsibility even when other
contexts coordinate execution.

**Acceptance criteria.** Confirmed lines and quantities are versioned;
fulfillment cannot exceed ordered quantity; cancellation follows current
fulfillment state; source facts are published idempotently for Inventory,
Operations, Finance, and Customer 360.

**Sources.** [Sales context](../../crates/contexts/sales/src/lib.rs),
[Sales API](../../crates/proto/proto/erpx/sales/v1/service.proto),
[operations boundaries](../../adr/ADR-008-operations-boundaries.md).

### REQ-SAL-003 — Accepted fulfillment facts

**Requirement.** ERPX shall accept traceable fulfillment facts against sales
order lines and shall expose partial and final fulfillment separately.

**Rationale.** Billing, revenue, and customer service depend on the actual
delivery state.

**Acceptance criteria.** Each fact identifies order, line, fulfillment ID,
quantity, type, and time; duplicate facts are idempotent; incompatible or
excess fulfillment is rejected; accepted facts publish stable integration
events.

**Sources.** [Sales context](../../crates/contexts/sales/src/lib.rs),
[sales integration contracts](../../crates/contexts/sales/src/integration/mod.rs).

### REQ-SAL-004 — Revenue arrangements

**Requirement.** ERPX shall create and activate a revenue arrangement with
explicit performance obligations and allocation before Finance recognizes
revenue.

**Rationale.** Commercial order value must be decomposed into auditable revenue
obligations.

**Acceptance criteria.** Obligations have unique IDs, definitions, allocation,
and fulfillment rules; activation requires valid source terms and approvals;
cancellation and completion are explicit; Finance consumes a versioned
arrangement fact rather than Sales ledger data.

**Sources.** [Sales context](../../crates/contexts/sales/src/lib.rs),
[Sales API](../../crates/proto/proto/erpx/sales/v1/service.proto),
[Finance context](../../crates/contexts/finance/src/lib.rs).

### REQ-SAL-005 — Governed commercial release

**Requirement.** ERPX shall prevent quote, order, or revenue-arrangement release
until applicable approval gates, entitlement, customer, credit, and scope
conditions are satisfied.

**Rationale.** High-value commitments need consistent control before downstream
execution.

**Acceptance criteria.** Release evaluates current source version and pinned
approval session; failed maker-checker or stale evidence blocks release;
successful release emits one traceable fact; a later rejection cannot be
represented as an approved state.

**Sources.** [Sales context](../../crates/contexts/sales/src/lib.rs),
[approval design](../../audits/APPROVAL-GOVERNANCE-PENCIL-2026-08-13/README.md).

## Point of Sale

### REQ-POS-001 — Register sale lifecycle

**Requirement.** ERPX shall manage point-of-sale transactions through create,
open, hold, resume, payment, complete, and void states at an authorized
register and branch.

**Rationale.** Front-line sales require fast operation without weakening scope
or accounting control.

**Acceptance criteria.** A sale is bound to register, branch, operator, lines,
currency, and version; only open sales accept payment; completed sales are
immutable; void follows defined state and authority rules.

**Sources.** [POS context](../../crates/contexts/pos/src/lib.rs),
[POS API](../../crates/proto/proto/erpx/pos/v1/service.proto).

### REQ-POS-002 — Payment recording

**Requirement.** ERPX shall record payment evidence by approved method without
storing prohibited payment instrument secrets.

**Rationale.** Payment traceability is required, while sensitive provider data
must remain outside ERPX presentation and logs.

**Acceptance criteria.** Payment identifies provider-safe reference, method,
amount, currency, and time; totals cannot exceed defined tender rules;
credential or full instrument data is rejected; duplicate provider references
do not create duplicate payment.

**Sources.** [POS context](../../crates/contexts/pos/src/lib.rs),
[POS API](../../crates/proto/proto/erpx/pos/v1/service.proto).

### REQ-POS-003 — Refund governance

**Requirement.** ERPX shall manage refunds through request, independent review
when required, approval or rejection, posting, and cancellation.

**Rationale.** Refunds affect cash, inventory, customer trust, and fraud risk.

**Acceptance criteria.** Refund lines reference an eligible completed sale;
refunded quantity and amount cannot exceed the original less prior refunds;
maker-checker and limits are enforced; posting emits traceable Finance and
Inventory facts.

**Sources.** [POS context](../../crates/contexts/pos/src/lib.rs),
[POS API](../../crates/proto/proto/erpx/pos/v1/service.proto),
[POS refund domain](../../crates/domain/src/pos_refund.rs).

### REQ-POS-004 — POS downstream facts

**Requirement.** ERPX shall publish completed sale and posted refund facts for
Inventory, Finance, Customer 360, and Operations without permitting those
consumers to alter the POS transaction.

**Rationale.** Downstream stock and accounting must converge to register
activity while preserving POS ownership.

**Acceptance criteria.** Facts are versioned, idempotent, branch-scoped, and
source-linked; consumers can replay them; reconciliation identifies any sale,
payment, stock, or journal mismatch; replay cannot duplicate a stock movement
or journal.

**Sources.** [POS context](../../crates/contexts/pos/src/lib.rs),
[POS projection worker](../../apps/worker/src/pos_projection.rs),
[integration contracts](../../crates/integration-contracts/src/lib.rs).

## Lifecycle-completion requirements

| ID | Title | Normative requirement | Rationale | Acceptance paths | Source | Scenarios |
|---|---|---|---|---|---|---|
| REQ-FIN-090 | Finance lifecycle coverage | ERPX shall govern setup, entry, approval, posting, settlement, correction, reversal, close, reopen, archive, and retention for Finance records. | Accounting history requires complete control. | Success: valid transition balances and receipts; Denial: unbalanced/closed-period mutation is rejected; Failure: partial journal/outbox write rolls back. | [FSD-003](../decisions/FSD-003-enterprise-suite-expansion.md) | E2E-06, E2E-41 |
| REQ-FIN-091 | Finance reconciliation | ERPX shall reconcile subledgers, bank, tax, Sales, Procurement, POS, Payroll, Projects, Assets, and provider facts to the ledger. | Financial completeness spans contexts. | Success: differences reach governed disposition; Denial: source context cannot mark ledger posted; Failure: imbalance blocks close. | [FSD-003](../decisions/FSD-003-enterprise-suite-expansion.md) | E2E-06, E2E-35, E2E-41 |
| REQ-SAL-090 | Sales lifecycle coverage | ERPX shall govern lead acceptance, quote, approval, acceptance, order, change, fulfillment, return, cancellation, close, and archive. | Lead-to-cash needs complete states. | Success: exact-version transitions emit receipts; Denial: invalid or unauthorized transition is rejected; Failure: partial handoff remains reconcilable. | [FSD-003](../decisions/FSD-003-enterprise-suite-expansion.md) | E2E-06, E2E-36 |
| REQ-SAL-091 | Sales reconciliation | ERPX shall reconcile customer, price, inventory, fulfillment, return, invoice, cash, revenue, commission, and commerce outcomes. | Sales truth crosses domains. | Success: every accepted source fact links once; Denial: Sales cannot post Finance/Inventory state; Failure: mismatch creates work. | [FSD-003](../decisions/FSD-003-enterprise-suite-expansion.md) | E2E-06, E2E-36 |
| REQ-POS-090 | POS lifecycle coverage | ERPX shall govern register, shift, sale, tender, receipt, void, return, refund, close, correction, and archive. | Store transactions need full accountable lifecycle. | Success: authorized online/offline-approved transition balances; Denial: unauthorized refund/void is rejected; Failure: uncertain payment reconciles before retry. | [FSD-003](../decisions/FSD-003-enterprise-suite-expansion.md) | E2E-11 |
| REQ-POS-091 | POS reconciliation | ERPX shall reconcile register, payment, stock, customer, tax, refund, cash, and Finance facts. | POS state cannot diverge from sources. | Success: shift closes with explained differences; Denial: cashier cannot fabricate provider success; Failure: mismatch blocks final close. | [FSD-003](../decisions/FSD-003-enterprise-suite-expansion.md) | E2E-11 |
