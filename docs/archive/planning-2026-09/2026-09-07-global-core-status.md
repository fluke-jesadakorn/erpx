# Global core implementation status — 2026-09-07

> Historical plan/report. Superseded for delivery scope and priorities by [the current production plan](../../production-plan.md). Runtime and implementation statements below describe their original date; see [the retention index](README.md) for current requirements.


The requested production upgrade is **partially implemented**. Nine modules now connect to real domain services; the other 22 retain generic workbenches or previews. None of the 31 modules is being declared fully production-complete. The global-core direction follows the user's clarification to avoid prioritizing a particular country.

## Implemented and exercised

- Replaced duplicate first-name fixtures with 60 distinct fictional full names, stable employee/person links, 18 development accounts and actual role scopes. Activation uses a hashed, expiring, single-use invitation; revocation is enforced on subsequent requests.
- Added typed product, party, warehouse, quote, sales-order and purchase-order CRUD. Approval, partial fulfillment, invoice creation and settlement are persisted transactions with optimistic versions, idempotency receipts, audit events and an outbox.
- Connected goods receipts to inventory/GRNI; deliveries to stock value/COGS; invoices to AR/AP, revenue and tax; settlements to cash and allocations. The office-furniture scenario leaves 15 desks worth USD 3,000 and a fully paid USD 1,872.50 customer invoice.
- Added manual journals and independent posting/reversal, service invoices, bank statement import and payment matching, chart-specific trial balances, and close controls. Original reversed entries remain in reports with the reversing entry.
- Added employee onboarding/edit/activation/archive, leave creation/submission/review/rejection/cancellation, compensation revisions, pay-group assignment, time submission/correction/approval and independently reviewed gross-pay previews.
- Connected expense claims to actual employee accounts, private merchant receipts, correction/resubmission, independent HR decisions, expense/input-tax/AP posting, partial and full reimbursement, bank matching and unpaid reversal. Cancelling an unsubmitted claim preserves its history. Work, People and Finance link to the shared claim.
- Added physical stock-count preparation, correction, submission, cancellation, independent finance posting and immutable review decisions. Quantity and value changes post atomically with inventory/variance journals. Concurrent counts, subsequent stock movements and backdated deliveries are guarded; zero and no-difference counts are supported.
- Added direct same-company warehouse transfers with source preparation and independent receiving review, corrections/cancellation, stock-snapshot guards and paired movements at preserved cost. Branch roles are checked against source and receiving warehouses.
- Home and Work now include role-specific expense, stock-count and transfer actions, with links to the owning document and shared To do/Waiting/Completed/All filters. Journal detail resolves employee names and links back to physical counts or expense claims.
- Reworked queue projection to handle older events without status and prevent delayed messages overwriting newer states. Failed deliveries use bounded retries and configured dead-letter queues.

Source comparison: [ERPNext, Frappe HR and Odoo workflow research](2026-09-06-production-workflows-research.md). The implementation adopts connected document flows and independent approval patterns; it does not claim feature parity with those ERPs.

## Verification evidence

- `bun run lint`: passed.
- `bunx tsc --noEmit`: passed; Next production build also type-checks.
- `bun run test:integration`: 7 files, 24 tests passed, including D1 atomic rollback/replay and delayed-event projection.
- `bun run test:business:local`: six persisted HTTP journeys passed on the Cloudflare preview at 8787 (commerce; identity/leave; ledger/bank/pay inputs; employee expense/receipt/review/accounting/reimbursement/reversal; physical count/stock/COGS/GL; warehouse transfer/receipt/valuation). Tests verify positive flows and denied self-approval, stale updates, duplicate commands, over-receipts and revoked access.
- `bun run seed:verify:local`: checks foreign keys, balanced posted journals, inventory movement/stock reconciliation, independent count and transfer posting, employee/payable consistency, allocation limits, statement arithmetic and R2 checksums. It is **not** a statutory, audit or complete historical GL/subledger certification.
- `bun run build:cloudflare`: Next.js and OpenNext Worker build passed. The tool emits an experimental Node.js middleware warning; this requires a runtime decision and deployment testing before launch.
- Jobs Worker Wrangler dry-run: passed. Queue projection is tested in Workers; live queue delivery, scheduled dispatch and failure recovery still need an environment acceptance run.
- Browser: created/activated Sophie Vermeer, created/submitted a real chair quotation, and created/edited/resubmitted a persisted time entry; inspected persisted employees, role menus, order/finance detail, ledger and bank matching on the built preview at 8787. Completed the expense browser journey: Mia created Olivia Grant’s EUR 48 stationery claim and uploaded its private receipt; Sam approved, posted and recorded reimbursement, with zero outstanding. Completed the stock-count browser journey: Mali created and corrected the monitor count, Nisa received the pending action in Home and posted 11 monitors worth USD 2,640. Inspected the linked inventory/variance journal and retained actor/review history.

## Module inventory

“Partial” means the listed flow has domain behavior. Generic record CRUD, saved forms, sample rows and route coverage do not constitute a completed ERP module.

| Module | Status | Current implementation | Required work |
| --- | --- | --- | --- |
| Brand | Workbench / preview | Generic persisted workspace records and/or preview pages; no complete domain workflow verified | Implement the module's typed records, invariants, workflow transitions, integrations and acceptance journeys |
| Commercial | Workbench / preview | Generic persisted workspace records and/or preview pages; no complete domain workflow verified | Implement the module's typed records, invariants, workflow transitions, integrations and acceptance journeys |
| Identity & Access | Partial domain implementation | Password login; employee/account linking; owner-issued activation; scoped role grants and revocation | SSO/MFA, verified email changes, recovery, invitation delivery, real tenant bootstrap/import |
| Organization | Workbench / preview | Generic persisted workspace records and/or preview pages; no complete domain workflow verified | Implement the module's typed records, invariants, workflow transitions, integrations and acceptance journeys |
| Platform Admin | Workbench / preview | Generic persisted workspace records and/or preview pages; no complete domain workflow verified | Implement the module's typed records, invariants, workflow transitions, integrations and acceptance journeys |
| Customer 360 | Partial domain implementation | Customer/supplier directory used by quotes, orders and invoices | Unified service/contact/consent history and customer analytics |
| Customer Authority | Workbench / preview | Generic persisted workspace records and/or preview pages; no complete domain workflow verified | Implement the module's typed records, invariants, workflow transitions, integrations and acceptance journeys |
| Support | Workbench / preview | Generic persisted workspace records and/or preview pages; no complete domain workflow verified | Implement the module's typed records, invariants, workflow transitions, integrations and acceptance journeys |
| Finance | Partial domain implementation | Journal preparation and independent posting, reversal, standalone invoices, settlement, statement import/matching, ledger reports, period controls and linked employee expenses/reimbursement | Credit/debit notes, payment execution, close acceptance scenarios, asset/revenue/lease/loan workflow hardening; reimbursement corrections after payment, multi-currency expense conversion; historical seed-ledger reconciliation |
| Point of Sale | Workbench / preview | Generic persisted workspace records and/or preview pages; no complete domain workflow verified | Implement the module's typed records, invariants, workflow transitions, integrations and acceptance journeys |
| Sales | Partial domain implementation | Quotation → approval → sales order → partial delivery → invoice → receipts | Returns, credit notes, price lists, discounts, reservation/credit policies, full approved-order cancellation |
| Assets | Workbench / preview | Generic persisted workspace records and/or preview pages; no complete domain workflow verified | Implement the module's typed records, invariants, workflow transitions, integrations and acceptance journeys |
| Inventory | Partial domain implementation | Products, warehouses, quantity/value movements, moving-average cost, physical count/adjustment approval and posting, direct warehouse transfers with independent receipt confirmation, negative-stock and backdating protection | Goods in transit/partial receiving, reservations, lot/serial tracking, landed cost, returns, count valuation evidence attachments, configurable variance mapping and opening-equity imports |
| Maintenance | Workbench / preview | Generic persisted workspace records and/or preview pages; no complete domain workflow verified | Implement the module's typed records, invariants, workflow transitions, integrations and acceptance journeys |
| Operations | Workbench / preview | Generic persisted workspace records and/or preview pages; no complete domain workflow verified | Implement the module's typed records, invariants, workflow transitions, integrations and acceptance journeys |
| Procurement | Partial domain implementation | Supplier → purchase order → independent approval → partial receipts → AP invoice → recorded settlement | Requisitions/RFQ, price/quantity variance policies, supplier returns and credit notes |
| Quality | Workbench / preview | Generic persisted workspace records and/or preview pages; no complete domain workflow verified | Implement the module's typed records, invariants, workflow transitions, integrations and acceptance journeys |
| Legal | Workbench / preview | Generic persisted workspace records and/or preview pages; no complete domain workflow verified | Implement the module's typed records, invariants, workflow transitions, integrations and acceptance journeys |
| People & Payroll | Partial domain implementation | Employee placement/lifecycle, linked roles, leave, time correction/approval, compensation approval, employee expense claims/review and gross-pay previews | Statutory payroll, proration, configurable overtime policy, employee advances, expense currency conversion, final pay, bank adapters and local filings |
| CRM | Partial domain implementation | Customer accounts and handoff to quotations | Leads, opportunities, activities, forecasting and pipeline transitions |
| Marketing | Workbench / preview | Generic persisted workspace records and/or preview pages; no complete domain workflow verified | Implement the module's typed records, invariants, workflow transitions, integrations and acceptance journeys |
| Manufacturing | Workbench / preview | Generic persisted workspace records and/or preview pages; no complete domain workflow verified | Implement the module's typed records, invariants, workflow transitions, integrations and acceptance journeys |
| Planning | Workbench / preview | Generic persisted workspace records and/or preview pages; no complete domain workflow verified | Implement the module's typed records, invariants, workflow transitions, integrations and acceptance journeys |
| Projects | Workbench / preview | Generic persisted workspace records and/or preview pages; no complete domain workflow verified | Implement the module's typed records, invariants, workflow transitions, integrations and acceptance journeys |
| E-commerce | Workbench / preview | Generic persisted workspace records and/or preview pages; no complete domain workflow verified | Implement the module's typed records, invariants, workflow transitions, integrations and acceptance journeys |
| Benefits | Workbench / preview | Generic persisted workspace records and/or preview pages; no complete domain workflow verified | Implement the module's typed records, invariants, workflow transitions, integrations and acceptance journeys |
| Learning | Workbench / preview | Generic persisted workspace records and/or preview pages; no complete domain workflow verified | Implement the module's typed records, invariants, workflow transitions, integrations and acceptance journeys |
| Talent | Workbench / preview | Generic persisted workspace records and/or preview pages; no complete domain workflow verified | Implement the module's typed records, invariants, workflow transitions, integrations and acceptance journeys |
| Field Service | Workbench / preview | Generic persisted workspace records and/or preview pages; no complete domain workflow verified | Implement the module's typed records, invariants, workflow transitions, integrations and acceptance journeys |
| Agents | Workbench / preview | Generic persisted workspace records and/or preview pages; no complete domain workflow verified | Implement the module's typed records, invariants, workflow transitions, integrations and acceptance journeys |
| Collaboration | Partial domain implementation | Persisted posts, comments, reactions, follows and Stories | Moderation, retention/export, all notification and cross-module handoff workflows |

## Production requirements still open

1. Complete the unimplemented business workflows above, especially returns/credits, goods in transit/reservations, payment corrections and domain-specific operations.
2. Reconcile historical synthetic invoices/payments to a fully linked ledger or replace them with approved opening imports. The new office-furniture and service-invoice scenarios have complete postings; older UI-volume fixtures do not prove accounting completeness. Unsupported fixture bank matches are reset to unmatched with snapshots preserved.
3. Configure real tenant identities, opening balances, master data and authorization boundaries. Development seeds are fictional and remote fixture seeding is blocked. No actual employer records have been imported.
4. Implement statutory pay/tax adapters, employment proration and configurable overtime rules where required. The current calculator is a gross-pay preview (hourly overtime currently uses 1.5); production creation/posting is blocked. Money workflows currently use two-decimal currencies; zero/three-decimal ISO currencies need currency-specific precision throughout.
5. Implement payment-provider/bank execution, statement adapters and verified payment confirmations. “Record settlement” records money already moved; it does not initiate a bank transfer. Generic seeded CSV exports are fixtures.
6. Complete identity recovery/SSO/MFA and verified email delivery; invitation activation is presently delivered as an owner-copied link. Cross-database account activation has a recoverable intermediate account state and must have operational recovery coverage.
7. Harden remaining legacy API services to the same atomic command protocol. Validate all scope boundaries, file cleanup, rate limiting, pagination and audit retention with adversarial tests and deployment-sized data.
8. Resolve the OpenNext middleware support warning; provision production D1/R2/Queues and secrets; exercise backup restore, migrations, queue DLQ recovery, observability, load tests and deployment rollback. No production deployment occurred.

## Capability inventory

This inventory is derived from `src/data/module-registry.json` and `liveDestination`. A routed capability remains partial until every described behavior is implemented and verified.

| Module | Capability | Domain connection |
| --- | --- | --- |
| Brand | Draft workspace | Domain action not implemented |
| Brand | Category locks | Domain action not implemented |
| Brand | Published identity | Domain action not implemented |
| Commercial | Invoices | Domain action not implemented |
| Commercial | Entitlements | Domain action not implemented |
| Commercial | Catalog plans | Domain action not implemented |
| Commercial | Subscriptions | Domain action not implemented |
| Commercial | Grace & recovery | Domain action not implemented |
| Identity & Access | Assignments | Partial connection: `people` |
| Identity & Access | Access reviews | Domain action not implemented |
| Identity & Access | People & principals | Partial connection: `people` |
| Identity & Access | Roles & permissions | Partial connection: `people` |
| Identity & Access | Directory connectors | Domain action not implemented |
| Identity & Access | Identity conflicts | Domain action not implemented |
| Organization | Organization tree | Domain action not implemented |
| Organization | Sharing | Domain action not implemented |
| Organization | Node lifecycle | Domain action not implemented |
| Organization | Business units | Domain action not implemented |
| Platform Admin | Provisioning | Domain action not implemented |
| Platform Admin | Holdings | Domain action not implemented |
| Platform Admin | Connectors | Domain action not implemented |
| Platform Admin | Infrastructure | Domain action not implemented |
| Platform Admin | Backup & recovery | Domain action not implemented |
| Platform Admin | Projections & events | Domain action not implemented |
| Platform Admin | Migration runs | Domain action not implemented |
| Platform Admin | Temporary access | Domain action not implemented |
| Platform Admin | Platform audit | Domain action not implemented |
| Customer 360 | Sales summary | Partial connection: `commerce?view=sales` |
| Customer 360 | Service timeline | Domain action not implemented |
| Customer 360 | Customer profile | Partial connection: `commerce?view=parties` |
| Customer 360 | Customer notes | Domain action not implemented |
| Customer Authority | Duplicate review | Domain action not implemented |
| Customer Authority | Access history | Domain action not implemented |
| Customer Authority | Authority bindings | Domain action not implemented |
| Customer Authority | Identity changes | Domain action not implemented |
| Support | Tickets | Domain action not implemented |
| Support | SLA watch | Domain action not implemented |
| Support | Incidents | Domain action not implemented |
| Support | Support sessions | Domain action not implemented |
| Support | Capabilities | Domain action not implemented |
| Support | Attachments | Domain action not implemented |
| Finance | Journals | Partial connection: `finance/ledger` |
| Finance | Payables | Partial connection: `finance/invoices?kind=payable` |
| Finance | Chart of accounts | Partial connection: `finance/ledger` |
| Finance | Payments | Partial connection: `finance/invoices` |
| Finance | Receivables & aging | Partial connection: `finance/invoices?kind=receivable` |
| Finance | Bank reconciliation | Partial connection: `finance/bank` |
| Finance | Reports | Partial connection: `finance/ledger` |
| Finance | Period close | Partial connection: `finance/ledger` |
| Finance | Opening balances | Domain action not implemented |
| Point of Sale | Refunds | Domain action not implemented |
| Point of Sale | Register session | Domain action not implemented |
| Point of Sale | Sales & baskets | Domain action not implemented |
| Point of Sale | Payments & receipts | Domain action not implemented |
| Point of Sale | Voids | Domain action not implemented |
| Sales | Quotes | Partial connection: `commerce?view=quote` |
| Sales | Orders | Partial connection: `commerce?view=sales` |
| Sales | Fulfillment | Partial connection: `commerce?view=sales` |
| Sales | Cancellations | Partial connection: `commerce?view=sales` |
| Sales | Revenue handoff | Partial connection: `commerce?view=sales` |
| Assets | Asset register | Domain action not implemented |
| Assets | Condition | Domain action not implemented |
| Assets | Placement | Domain action not implemented |
| Assets | Maintenance lifecycle | Domain action not implemented |
| Assets | Retirement | Domain action not implemented |
| Inventory | Availability | Partial connection: `commerce?view=stock` |
| Inventory | Reservations | Domain action not implemented |
| Inventory | Stock register | Partial connection: `commerce?view=stock` |
| Inventory | Receipts | Partial connection: `commerce?view=purchase` |
| Inventory | Issues & movements | Partial connection: `commerce?view=stock` and `stock-counts` |
| Inventory | Physical count and adjustment | Domain workflow: create/edit/submit/return/cancel/post; stock and journal commit together |
| Maintenance | Work orders | Domain action not implemented |
| Maintenance | Downtime | Domain action not implemented |
| Maintenance | Blocked work | Domain action not implemented |
| Maintenance | Completion | Domain action not implemented |
| Maintenance | Maintenance history | Domain action not implemented |
| Operations | Exception queue | Domain action not implemented |
| Operations | Escalations | Domain action not implemented |
| Operations | Approval decisions | Domain action not implemented |
| Operations | Resolution | Domain action not implemented |
| Operations | Queue policies | Domain action not implemented |
| Operations | Operations summary | Domain action not implemented |
| Procurement | Requisitions | Domain action not implemented |
| Procurement | Purchase orders | Partial connection: `commerce?view=purchase` |
| Procurement | Goods receipts | Partial connection: `commerce?view=purchase` |
| Procurement | Invoice match | Partial connection: `commerce?view=purchase` |
| Procurement | Supplier history | Partial connection: `commerce?view=parties` |
| Quality | Inspections | Domain action not implemented |
| Quality | Results | Domain action not implemented |
| Quality | Release gates | Domain action not implemented |
| Quality | Cancellation | Domain action not implemented |
| Legal | Contracts | Domain action not implemented |
| Legal | Documents | Domain action not implemented |
| Legal | Legal holds | Domain action not implemented |
| Legal | Document access | Domain action not implemented |
| Legal | Archive | Domain action not implemented |
| People & Payroll | Employees | Partial connection: `people` |
| People & Payroll | Payroll summaries | Partial connection: `people` |
| People & Payroll | Leave requests | Partial connection: `work` |
| People & Payroll | Expenses | Partial connection: `expenses` |
| People & Payroll | Payroll runs | Partial connection: `people` |
| CRM | Accounts | Partial connection: `commerce?view=parties` |
| CRM | Activities | Domain action not implemented |
| CRM | Leads | Domain action not implemented |
| CRM | Opportunities | Domain action not implemented |
| CRM | Sales handoff | Partial connection: `commerce?view=quote` |
| Marketing | Forms & profiles | Domain action not implemented |
| Marketing | Content & previews | Domain action not implemented |
| Marketing | Domains | Domain action not implemented |
| Marketing | Provider readiness | Domain action not implemented |
| Marketing | Lead candidates | Domain action not implemented |
| Marketing | Engagement | Domain action not implemented |
| Manufacturing | Orders | Domain action not implemented |
| Manufacturing | Material readiness | Domain action not implemented |
| Manufacturing | Quality disposition | Domain action not implemented |
| Manufacturing | Specifications | Domain action not implemented |
| Manufacturing | Labor & output | Domain action not implemented |
| Manufacturing | Costs & genealogy | Domain action not implemented |
| Planning | Scenarios | Domain action not implemented |
| Planning | Shortages | Domain action not implemented |
| Planning | Recommendations | Domain action not implemented |
| Planning | Capacity | Domain action not implemented |
| Planning | Plan comparison | Domain action not implemented |
| Projects | Dependencies | Domain action not implemented |
| Projects | Projects & WBS | Domain action not implemented |
| Projects | Handoffs | Domain action not implemented |
| Projects | Time & costs | Domain action not implemented |
| Projects | Attachments | Domain action not implemented |
| Projects | Project graph | Domain action not implemented |
| E-commerce | Channels | Domain action not implemented |
| E-commerce | Listings | Domain action not implemented |
| E-commerce | Returns | Domain action not implemented |
| E-commerce | Carts | Domain action not implemented |
| E-commerce | Checkout | Domain action not implemented |
| E-commerce | Fulfillment | Domain action not implemented |
| Benefits | Dependents | Domain action not implemented |
| Benefits | Coverage | Domain action not implemented |
| Benefits | Plans | Domain action not implemented |
| Benefits | Enrollment windows | Domain action not implemented |
| Benefits | Provider & payroll | Domain action not implemented |
| Learning | Course catalog | Domain action not implemented |
| Learning | Assignments | Domain action not implemented |
| Learning | Attendance & results | Domain action not implemented |
| Learning | Completion | Domain action not implemented |
| Learning | Eligibility handoff | Domain action not implemented |
| Talent | Requisitions | Domain action not implemented |
| Talent | Applications | Domain action not implemented |
| Talent | Offers | Domain action not implemented |
| Talent | Candidates & privacy | Domain action not implemented |
| Talent | Hire handoffs | Domain action not implemented |
| Talent | Proposals | Domain action not implemented |
| Field Service | Service orders | Domain action not implemented |
| Field Service | Agreements | Domain action not implemented |
| Field Service | Dispatch | Domain action not implemented |
| Field Service | Visits | Domain action not implemented |
| Field Service | Exceptions | Domain action not implemented |
| Field Service | Billing release | Domain action not implemented |
| Agents | Agent catalog | Domain action not implemented |
| Agents | Versions | Domain action not implemented |
| Agents | Assignments | Domain action not implemented |
| Agents | Pools & routing | Domain action not implemented |
| Agents | Private sessions | Domain action not implemented |
| Agents | Field suggestions | Domain action not implemented |
| Agents | Invocation evidence | Domain action not implemented |
| Collaboration | Threads | Domain action not implemented |
| Collaboration | Mentions | Domain action not implemented |
| Collaboration | Comments & moderation | Partial connection: `feed` |
| Collaboration | Following & reactions | Partial connection: `feed` |
| Collaboration | Topics & feed | Partial connection: `feed` |
| Collaboration | Attachments | Domain action not implemented |
| Collaboration | Action cards | Domain action not implemented |
| Collaboration | Record relations | Domain action not implemented |

## Physical count boundaries

Counts currently adjust each warehouse/product at its recorded moving-average cost. A positive quantity without recorded stock requires a verified unit cost, and the offset is the configured operations chart’s active expense account with code `5400` (stock count variance). This is a count discrepancy workflow; a complete opening-balance migration must separately establish the correct opening-equity balances. Posted counts are retained and corrected by a later count. Goods in transit, partial receiving, stock reservations, lot/serial tracking, valuation supporting-document attachments and configurable account mapping remain open. The current same-day ordering uses the committed stock version; dates before a later recorded movement are rejected rather than automatically reposting historical valuation.

Browser evidence records: expense `expense-2e9fcbdf-d90f-43e9-be90-ca938a4dfed7`; physical count `count-35c08740-41c0-4203-b4ec-8a833966db7a`; journal `journal-246df9ae-7536-4561-a0f1-ab154974bbc4`. All records are fictional local development examples, not imported employer or financial data.

Direct-transfer verification also checks that each product has exactly two opposite movement entries and that total company quantity/value remains unchanged. A transfer is a record of completed receipt; it does not dispatch a shipment, reserve stock while awaiting review, or track goods in transit.

Browser transfer evidence: `transfer-76de4d30-d9b8-4e83-a7a2-ec5392564d95` moved four monitors worth USD 960 from distribution to staging after Nisa’s independent receipt confirmation. Seven monitors worth USD 1,680 remain at distribution; four worth USD 960 are at staging. Company inventory remains eleven monitors worth USD 2,640.
