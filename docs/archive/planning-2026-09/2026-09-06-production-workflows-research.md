# Production workflow research — 2026-09-06

> Historical plan/report. Superseded for delivery scope and priorities by [the current production plan](../../production-plan.md). Runtime and implementation statements below describe their original date; see [the retention index](README.md) for current requirements.


This note translates first-party ERPNext/Frappe HR and Odoo workflows into ERPX acceptance criteria. The checkboxes are proposed verification work, not a claim that the implementation passes. Findings describe source files inspected on 2026-09-06; implementation may be changing concurrently.

## Current ERPX boundary

- `src/server/auth.ts` already uses Better Auth password authentication, sessions and disabled public signup. The missing identity work is the operational lifecycle around account creation, employee linkage, roles and deactivation, rather than a replacement login screen.
- `src/server/people-payroll.ts` owns people, engagements, compensation and payroll/payment batches. A person has an optional `user_id`. This is the right separation to preserve while making it usable.
- `src/server/records.ts` persists generic fields and the shared `draft → in_review → approved → completed` lifecycle. Persistence alone does not give Sales or Operations inventory movements, invoice accounting or settlement semantics. Finance already rejects generic-owner writes.
- `src/server/finance.ts` contains authoritative journals, invoices and payments. At inspection, `postInvoice` creates two gross-total journal lines, which needs explicit tax treatment when `tax_minor` is nonzero.
- Existing launch gaps remain documented in [the full-stack plan](2026-09-06-mockup-to-fullstack.md). A working form, a successful build, or a populated dashboard is evidence for only that behavior.

## 1. People, accounts and roles

Frappe HR keeps employment details in Employee and can link or create a separate User, including after employee creation. Employee data includes company, department, branch, reporting manager and dates. The default user-creation flow restricts access to the employee's own record and company. [Frappe HR Employee](https://docs.frappe.io/hr/employee)

Frappe defines User as an authenticated actor and Role as permitted actions on a document type; create, read, write, submit, cancel and other operations are distinct permissions. It additionally supports user-level restrictions on linked records. [Frappe Users and Permissions](https://docs.frappe.io/framework/user/en/basics/users-and-permissions)

ERPX acceptance:

- [ ] Create, inspect, edit and archive a person through People; display full name, employee code, job, department, manager, employment status and account status. Use stable IDs for identity. Repeated names can be legitimate and must not be a database uniqueness rule.
- [ ] Account provisioning creates exactly one account/principal, an explicit person link and scoped assignments. Account email is normalized and unique; an employee can exist without login access, and an external authorized user can exist without employment.
- [ ] Editing a job title does not grant a system role. Role changes require an authorized identity administrator and record actor, assignment, prior/new values and time.
- [ ] Linking an existing user validates that it is eligible in the Holding and cannot silently attach another person's account. Retries return the original result without duplicated accounts or people.
- [ ] Sign in as the new account, select only its own valid assignments, create an allowed record and receive a server denial for a forbidden action. Deep links and direct API requests obey the same scope.
- [ ] Suspend an account or revoke its assignment and verify an existing session loses that access. Ending one company engagement must not indiscriminately delete unrelated assignments.
- [ ] Preserve employment history and historic audit attribution when a person leaves; separate account deactivation from deletion of business records.

## 2. Sales: quote to cash

ERPNext's standard selling sequence is quotation, sales order, delivery note, sales invoice and payment entry. A submitted order records commitment; a direct invoice can be appropriate for service sales where delivery is unnecessary. Required masters include company, customer and item, with warehouse required for stock delivery. [ERPNext Sales Order](https://docs.frappe.io/erpnext/sales-order)

An invoice posts receivable, revenue and applicable tax; a draft has no ledger effect. Creating it from an order or delivery maintains upstream billing progress. A payment decreases outstanding receivables when properly allocated. [ERPNext Sales Invoice](https://docs.frappe.io/erpnext/sales-invoice)

ERPX acceptance:

- [ ] Manage customers and catalog items with stable IDs, active status, prices, units, currency and stock/service classification.
- [ ] Create and revise a quote with quantity, unit price, discounts, tax treatment and validity; server-calculated totals must persist and match the displayed totals.
- [ ] Accept the quote into one traceable order; retain quote revision and customer identity instead of copying unrelated display fields.
- [ ] Deliver a partial quantity from an identified warehouse and show ordered, delivered, returned and remaining quantities per line. Reject overdelivery unless a documented policy explicitly permits it.
- [ ] Generate an invoice for the eligible quantity or completed service; prevent repeat invoicing of the same source quantity and provide links back to the order/delivery.
- [ ] Post a taxed invoice to receivables, net revenue and a tax-control account; do not credit tax to revenue. Verify balanced entries and the relevant reporting period.
- [ ] Record a partial receipt, allocate it to the correct posted invoice, then settle the remainder. Reject cross-customer, cross-company and unsupported cross-currency allocation.
- [ ] Return goods and issue a linked credit note independently of refund settlement; order, stock, invoice and payment states remain reconcilable.

## 3. Procurement and warehouse operations

Odoo distinguishes billing on ordered quantities from billing on received quantities. Under received-quantity control, vendor bills follow partial or complete receipts; its three-way matching compares purchase, receipt and billing and is intended for this policy. [Odoo Manage vendor bills](https://www.odoo.com/documentation/18.0/applications/inventory_and_mrp/purchase/manage_deals/manage.html)

ERPX acceptance:

- [ ] Create supplier, warehouse and stock item masters, then a purchase order with quantities, prices, dates and approval evidence.
- [ ] Receive part of the order and persist a stock movement with source PO line, warehouse, actor and timestamp. The receipt updates remaining quantities exactly once.
- [ ] Track stock through movement records; stock-on-hand is derived from those movements rather than an editable dashboard total. Reject unsupported negative stock and incompatible units.
- [ ] Record a vendor bill against eligible ordered/received quantities according to an explicit control policy. Surface quantity/price discrepancies for review before posting or payment.
- [ ] Preserve supplier invoice reference and prevent duplicate supplier invoices within the relevant company and supplier scope.
- [ ] Post the payable and applicable expense/inventory/tax-control amounts, then allocate an independently authorized supplier payment.
- [ ] Process a supplier return with a linked reverse stock movement and supplier credit, without deleting the original receipt.
- [ ] Separate customer shipment from customer invoicing and supplier receipt from supplier billing; a shared status toggle cannot stand in for these four operations.

## 4. Leave and payroll

Frappe HR leave applications use employee, leave type, dates, balance/allocation and an approver. The approver can come from the employee or department; approval access is a separate permission concern. Applications have approval/rejection/cancellation outcomes, and payroll already processed for a period restricts subsequent leave submission. [Frappe HR Leave Application](https://docs.frappe.io/hr/leave-application), [Employee-wise Leave Approver](https://docs.frappe.io/hr/leave-management/help-articles/setting-employee-wise-leave-approver)

ERPX acceptance:

- [ ] Employee login resolves to the right person/engagement for self-service; show leave policy, balance, holiday calendar and pending requests.
- [ ] Validate date range, overlap, half-day rules and sufficient entitlement on the server; show the calculated days before submission.
- [ ] Route a request to the assigned eligible approver, reject self-approval, and enforce company/department scope even through direct API calls.
- [ ] Approval deducts entitlement once; cancellation restores it once. A repeated command or stale approval cannot double-count leave.
- [ ] Approved time/leave and effective compensation feed the correct pay period, with locked-period corrections represented explicitly.

Frappe HR payroll builds salary slips for selected employees, checks existing slips to avoid regeneration, and separates review/submission (salary accrual) from salary payment. It supports expense allocation by cost center and treats individual salary privacy as distinct from aggregate finance posting. [Frappe HR Payroll Entry](https://docs.frappe.io/hr/payroll-entry)

- [ ] A payroll run records company, pay group, period, compensation versions and calculation inputs; recalculate only through a visible lifecycle.
- [ ] Provide itemized employee results and validation exceptions, independent approval, payable/expense posting, payment-batch export and confirmation reconciliation.
- [ ] An exported file is not a settled payment. Failed and partially confirmed payments retain outstanding amounts.
- [ ] Employees see only their own payslip; HR/payroll roles and aggregate finance access remain separately controlled.

Odoo documents payroll localization as country-specific handling of taxes, contributions, benefits, holidays, termination and reporting obligations. A generic formula or currency selection is not evidence of jurisdiction compliance. [Odoo Payroll localizations](https://www.odoo.com/documentation/19.0/applications/hr/payroll/payroll_localizations.html)

- [ ] Keep synthetic payroll clearly marked as preview until the selected jurisdiction pack, effective dates, employer setup and statutory fixtures have been validated. Require those prerequisites in the posting/export APIs, not only in UI text.
- [ ] Production acceptance names the country and region, legal entity, pay frequencies, tax year, tax/contribution rules, reporting outputs and responsible payroll reviewer. This research does not validate any country's payroll rules.

## 5. Finance: posting, correction and settlement

ERPNext Payment Entry records cash/bank movement and allocations, including partial payments, advances and payments covering several invoices. Unallocated money can be reconciled later; an order is a commitment while an invoice creates an amount due. [ERPNext Payment Entry](https://docs.frappe.io/erpnext/payment-entry)

ERPNext's immutable ledger retains original entries and adds reversal rows on cancellation, preserving links to the source posting. Odoo's credit-note flow likewise creates reversing journal entries and permits partial corrections; stock returns and actual refunds remain separate actions. [ERPNext Immutable Ledger](https://docs.frappe.io/erpnext/immutable-ledger-in-erpnext), [Odoo Credit notes and refunds](https://www.odoo.com/documentation/19.0/applications/finance/accounting/customer_invoices/credit_notes.html)

ERPX acceptance:

- [ ] Draft edits do not change the ledger. Posting validates company, chart, account category, currency, exact minor-unit totals and open period.
- [ ] Posting an invoice or payment and changing its source status is recoverable and exactly-once under retries and interruption; concurrent calls cannot post it twice.
- [ ] Posted document amounts and ledger lines cannot be silently edited or archived; correction creates a linked reversal/credit with reason and permitted accounting date.
- [ ] Invoice outstanding equals posted amount minus valid allocations and credits. Payment unallocated amount equals recorded payment minus valid allocations.
- [ ] An allocated receipt/payment creates the corresponding bank and receivable/payable ledger effect; a UI settlement label cannot substitute for journal evidence.
- [ ] Trial balance balances, receivable/payable aging agrees with control accounts, and reports drill down to source documents and decisions.
- [ ] Close/reopen uses explicit role permissions and separate review where required; historical audit evidence stays readable after records/accounts are inactive.

Odoo documents optional hash chaining of secured accounting entries as an integrity mechanism, with each hash incorporating its predecessor. This is a useful later hardening reference; a hash alone does not prove all authorization or accounting controls. [Odoo Data inalterability check report](https://www.odoo.com/documentation/19.0/applications/finance/accounting/reporting/data_inalterability.html)

## 6. Development data and browser acceptance

The following are ERPX delivery recommendations, not claims copied from another ERP:

- [ ] Seed one coherent fictional business with full names, stable employee/customer/supplier/item IDs, `.test` email addresses, deliberate roles, manager relationships and dates. Make fictional provenance visible in development and keep real organizational imports separate.
- [ ] Re-running the development seed reconciles its owned fixture records without multiplying people or resetting user-created business records. Do not assume identical names are identical people.
- [ ] Seed an actual linked business case: purchase and receive 20 units; sell and deliver 5; invoice them; partially collect and then settle. Each step produces its real owning document, movement, accounting or allocation records.
- [ ] Seed a people case: hire an employee, provision login, submit leave, approve as the manager, and preview a pay period using that employee's effective compensation.
- [ ] Import actual organization records only from supplied, authorized sources with field mapping, validation, duplicate review and an import report. Invented names and generated payroll values must never be presented as real employees or compliant pay results.
- [ ] Browser-check every shipped action as its intended role, then reload, navigate to the linked module and verify persistence and downstream effects. Include validation errors, cancel/archive behavior, permission denial and stale/retry cases.
- [ ] Inventory remaining module actions explicitly. A module using generic record CRUD can be useful, but its domain capability is complete only when owner-specific effects and acceptance evidence exist.

Recommended implementation order: identity and coherent seeds; customer/supplier/item masters; sales and procurement document ownership; stock movements; invoice/payment integration; leave self-service; payroll jurisdiction-backed production gates; remaining module-specific engines and operational launch rehearsal.


## 7. Expense continuation — 2026-09-07

Frappe HR links an expense claim to an employee, expense category, tax amounts, approver and company payable account. It books the expense separately from its payment and permits allocation against outstanding employee claims. This supports ERPX's shared HR/finance claim rather than treating an export file as reimbursement. [Frappe HR Expense Claim](https://docs.frappe.io/hr/expense-claim)

ERPX now implements claim creation/correction/cancellation, mandatory private receipts, independent scoped review with decision reasons, an employee payable and net/tax posting, partial/full recorded reimbursement, bank matching and reversal before reimbursement. The reversal and stricter separation rules are ERPX design choices, not assertions about identical behavior in Frappe HR. `scripts/verify-expense-workflows.ts` exercises these through local HTTP and checks the resulting D1/R2 state.

Remaining expense work includes company-currency conversion, employee advances, payment-provider execution and corrections after reimbursement. Browser acceptance was completed on 2026-09-07 after the Mac became available: claim creation, receipt upload, independent approval, posting and reimbursement were exercised on the Cloudflare preview.

## 8. Physical stock counts — 2026-09-07

ERPNext’s stock reconciliation records a warehouse, item, physical quantity and valuation rate to reconcile recorded stock. Its perpetual-inventory documentation relates stock value changes to general-ledger balances. [ERPNext Stock Reconciliation](https://docs.frappe.io/erpnext/stock-reconciliation), [Accounting of Inventory Stock](https://docs.frappe.io/erpnext/accounting-of-inventory-stock).

ERPX applies this pattern with a saved stock snapshot, preparer-owned drafts, independent finance review, mandatory decision reasons, atomic quantity/value/journal posting and an append-only movement ledger. Positive stock without an existing valuation requires a verified acquisition rate; existing stock uses its recorded moving-average value. A later stock change makes an outstanding count stale. ERPX rejects earlier-dated adjustments or deliveries rather than attempting automatic historical valuation reposting. These approval and concurrency choices are ERPX design decisions, not claims of exact ERPNext behavior.

The browser monitor case and `scripts/verify-inventory-counts.ts` cover the persisted implementation. The HTTP journey checks opening snapshots, average-cost shrinkage, two simultaneous competing posts, returned counts/recounts, zero stock, no-difference counts, sales delivery/COGS, period and scope denials, and stock-to-GL reconciliation. Count differences currently use stock-variance account 5400; formal opening-equity imports, landed costs and valuation evidence attachments remain separate work.

## 9. Warehouse transfers — 2026-09-07

ERPNext distinguishes a material transfer between source and target warehouses from a goods-in-transit workflow, which adds an intermediate transit warehouse and a later receiving entry. [ERPNext Stock Entry](https://docs.frappe.io/erpnext/stock-entry), [Material Transfers and Goods in Transit](https://docs.frappe.io/erpnext/how-to-manage-material-transfers-and-goods-in-transit).

ERPX now implements completed direct transfers within one company: the source operator prepares quantities, an independent receiving manager or holding finance approver confirms receipt, and both warehouse movements commit atomically at the source moving-average cost. The target keeps its existing value plus the transferred value. With the current shared company inventory account, there is no net company general-ledger change. Draft corrections, return decisions, cancellation, branch scope, stale source stock and historical-date guards use the same command protocol as counts. Goods in transit, dispatch/partial receipt and cross-company transfers remain separate unimplemented flows.
